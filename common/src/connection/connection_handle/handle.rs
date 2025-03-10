use crate::connection::client_pool::client_pool::ClientPool;
use crate::connection::connection_handle::backup_init::listen_and_synchronize;
use crate::connection::connection_handle::channel::{AliveStatus, AliveStatusNotifier, AliveValue, ConnectionTransmitters};
use crate::connection::connection_handle::client_init::listen_for_controller_loop;
use crate::connection::connection_handle::handle::ConnectionState::{Connected, Disconnected};
use crate::connection::connection_handle::handle::MessageSendError::{HandleDisconnected, HandleKilled, KeepAliveTooSoon};
use crate::connection::connection_handle::message_sender::MessageSender;
use crate::connection::constants::{MESSAGE_POLLING_PERIOD, SEND_KEEP_ALIVE_PERIOD, TCP_TIMEOUT};
use crate::connection::controller_state::ControllerStateNotifier;
use crate::messages::Message::KeepAlive;
use crate::messages::{Message, TimedMessage, DEFAULT_MESSAGE, MESSAGE_SIZE};
use crossbeam_channel::{select_biased, tick, unbounded, Receiver, Sender};
use faulted::{is_faulted, set_to_faulted};
use log::log_client::{Logger, ReliableLogSender};
use log::LogLevel;
use std::cmp::Ordering;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock};
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};
use ConnectionState::Killed;

pub type ConnectionIdentifier = u8;

pub(in super::super::super::connection) enum ConnectionState {
    /// A handle is considered [Connected](Pending) when it contains an active and alive
    /// [TcpStream].
    Connected {
        /// This field is used only when the connection handle is used to connect to a controller.
        /// For an elevator client, this value is the id of the current controller, it is used for reconciliation purpose.
        /// For a backup node, it allows synchronisation with the master
        ///
        /// A client seeing two master controllers will always pick the one of lowest id.
        controller_id: ConnectionIdentifier,
        stream: TcpStream
    },
    /// A handle is considered [Disconnected](Disconnected) when it does not contain an active and alive
    /// [TcpStream], but connection is still possible later in time.
    Disconnected,

    /// A handle is considered [Killed](Killed) when it is definitely disconnected. Associated threads are going
    /// or are already killed.
    Killed
}

#[inline]
fn conditional_faulting(
    state: &Arc<RwLock<ConnectionState>>,
    reason: &str
) {
    if let Killed = state.read().unwrap().deref() {} else { set_to_faulted(reason); }
}

pub struct ConnectionHandleMutator {
    connection_state: Arc<RwLock<ConnectionState>>,
    alive_status_notifier: AliveStatusNotifier,
    logger: ReliableLogSender
}

impl Clone for ConnectionHandleMutator {
    fn clone(&self) -> Self {
        ConnectionHandleMutator {
            connection_state: self.connection_state.clone(),
            alive_status_notifier: self.alive_status_notifier.clone(),
            logger: self.logger.clone(),
        }
    }
}

impl ConnectionHandleMutator {
    pub fn logger(&self) -> &ReliableLogSender {
        &self.logger
    }

    fn connect_to(&self, address: SocketAddr, controller_id: ConnectionIdentifier) {
        match TcpStream::connect(address) {
            Ok(stream) => {
                if let Err(err) = stream.set_nonblocking(true) {
                    set_to_faulted(&format!("Cannot set stream to nonblocking: {err}"))
                }
                self.connect_with(stream, controller_id);
            }
            Err(_) => {
                *self.connection_state.write().unwrap() = Disconnected;
                self.alive_status_notifier.disconnected();
                self.logger.send(&format!("Cannot connect to {address}"), LogLevel::INFO);
            }
        }
    }

    pub fn connect_with(&self, stream: TcpStream, controller_id: ConnectionIdentifier) {
        let message = &format!("Connected with controller at {:?}", stream.peer_addr());

        *self.connection_state.write().unwrap() = Connected {
            controller_id,
            stream,
        };
        self.alive_status_notifier.connected();
        self.logger.send(message, LogLevel::INFO);
    }

    pub fn try_to_connect_to(&self, address: SocketAddr, controller_id: ConnectionIdentifier) {
        // Check for already established connection
        if self.is_connected_to_controller(address, controller_id) { return; }

        // If current connection is in fact dead, try to establish new connection
        self.disconnect(); // Preventive disconnection.
        self.connect_to(address, controller_id);
    }

    pub fn is_connected(&self) -> bool {
        if let Connected { .. } = *self.connection_state.read().unwrap() {
            true
        } else { false }
    }

    fn is_connected_to_controller(&self, address: SocketAddr, controller_id: ConnectionIdentifier) -> bool {
        if let Connected { stream, controller_id: current_controller_id } = self.connection_state.read().unwrap().deref() {
            stream.peer_addr().is_ok_and(|current_socket_addr| {
                current_socket_addr == address || // If we share the same address, we are still connected to the same host
                    // If changing controller is required
                    match current_controller_id.cmp(&controller_id) {
                        // Keep the current connection.
                        Ordering::Less | Ordering::Equal => true,
                        // In this case, the policy is to stick with the other controller, of lower id.
                        Ordering::Greater => false
                    }
            })
        } else { false }
    }

    pub fn disconnect(&self) {
        *self.connection_state.write().unwrap() = Disconnected;
        self.alive_status_notifier.disconnected();
    }
}

pub struct ConnectionHandle {
    connection_state: Arc<RwLock<ConnectionState>>,
    channels: ConnectionTransmitters,
    is_temporary: bool,
    logger: ReliableLogSender
}

impl ConnectionHandle {
    fn uninitialized(logger: ReliableLogSender, is_temporary: bool) -> Self {
        let mut connection = Self {
            connection_state: Arc::new(RwLock::new(Disconnected)),
            channels: ConnectionTransmitters::init(),
            logger,
            is_temporary
        };

        let (msg_tx, msg_rx) = connection.messages_channels();
        connection.channels.populate(msg_tx, msg_rx);

        connection
    }
    
    fn get_handle_mutator(&self) -> ConnectionHandleMutator {
        ConnectionHandleMutator {
            connection_state: self.connection_state.clone(),
            alive_status_notifier: self.alive_status_notifier(),
            logger: self.logger.clone(),
        }
    }

    pub(in super::super::super::connection) fn new_temporary_connection_handler(
        stream: TcpStream,
        logger: ReliableLogSender,
    ) -> Self {
        let connection = Self::uninitialized(logger, true);

        if let Err(_non_blocking_error) = stream.set_nonblocking(true) {
            set_to_faulted("Cannot set stream to non blocking");
        }

        connection.connect_to(stream, 0, false);

        connection
    }

    pub fn new_backup_connection_handler(
        controller_state: ControllerStateNotifier,
        client_pool: ClientPool,
        logger: ReliableLogSender
    ) -> ConnectionHandle {
        let handle =  Self::uninitialized(logger, false);

        listen_and_synchronize(
            handle.get_handle_mutator(),
            controller_state,
            client_pool
        );

        handle
    }

    pub fn new_server_side_connection_handler(client_id: ConnectionIdentifier) -> Self {
        Self::uninitialized(
            Logger::get_sender(format!("[ClientPool][{client_id}][TCP]")),
            false
        )
    }

    pub fn new_client_side_connection_handler(client_id: ConnectionIdentifier) -> Self {
        let handle = Self::uninitialized(
            Logger::get_sender(format!("[Client][{client_id}]")),
            false
        );
        listen_for_controller_loop(handle.get_handle_mutator());
        handle
    }

    pub fn take_receiver(&mut self) -> Receiver<Message> {
        self.channels.take_receiver()
    }
    pub fn take_sender(&mut self) -> MessageSender {
        MessageSender::from(self.channels.take_sender())
    }

    pub(in super::super::super::connection) fn borrow_sender(&self) -> &Sender<TimedMessage> {
        self.channels.borrow_sender()
    }

    pub(in super::super::super::connection) fn borrow_receiver(&self) -> &Receiver<Message> {
        self.channels.borrow_receiver()
    }

    pub(super) fn alive_status_notifier(&self) -> AliveStatusNotifier {
        self.channels.get_alive_notifier_instance()
    }

    pub(in super::super::super::connection) fn extract_stream_and_kill(self) -> Option<TcpStream> {
        let stream = match self.connection_state.write().unwrap().deref() {
            Connected { stream, .. } => {
                match stream.try_clone() {
                    Ok(stream) => Some(stream),
                    Err(_) => None
                }
            }
            Disconnected => None,

            // Already dead, ignore killing order.
            Killed => return None
        };

        self.kill();
        stream
    }

    pub(in super::super::super::connection) fn kill(self) {
        assert_eq!(self.is_temporary, true, "Cannot kill a client connection handle.");
        *self.connection_state.write().unwrap() = Killed;
        self.channels.disconnected();
    }

    pub(in super::super::super::connection) fn disconnect(&self) {
        *self.connection_state.write().unwrap() = Disconnected;
        self.channels.disconnected();
    }

    pub(in super::super::super::connection) fn connect_to(&self, stream: TcpStream, controller_id: u8, is_authenticated: bool) {
        *self.connection_state.write().unwrap() = Connected { controller_id, stream };
        if is_authenticated {
            self.channels.authenticated();
        } else {
            self.channels.connected();
        }

    }

    pub(in super::super::super::connection) fn is_connected(&self) -> bool {
        match *self.connection_state.read().unwrap() {
            Connected { .. } => true,
            Disconnected | Killed => false
        }
    }

    fn message_recv_loop(&mut self) -> Receiver<Message> {
        let connection_handle_mutator = self.get_handle_mutator();

        let (message_tcp_recv_tx, message_tcp_recv_rx) = unbounded();
        let mut raw_message_buffer = DEFAULT_MESSAGE;
        let mut was_connected = false;
        let mut last_message_timestamp: Instant = Instant::now();

        spawn(move || {
            let logger = connection_handle_mutator.logger();
            'message_receive_loop: loop {
                if is_faulted() { break 'message_receive_loop }

                let mut writeable_stream
                    = connection_handle_mutator.connection_state.write().unwrap();

                match writeable_stream.deref_mut() {
                    Connected { stream, .. } => {
                        if !was_connected {
                            was_connected = true;
                            last_message_timestamp = Instant::now();
                        }

                        let read_result = stream.read(&mut raw_message_buffer);
                        drop(writeable_stream);

                        match read_result {
                            // Valid message.
                            Ok(MESSAGE_SIZE) => {
                                let message = Message::decode_message(&raw_message_buffer);

                                last_message_timestamp = Instant::now();

                                if let Err(_channel_severed) = message_tcp_recv_tx.send(message) {
                                    conditional_faulting(
                                        &connection_handle_mutator.connection_state,
                                        "Unable to transmit packet from the network since the channel broke");
                                    break 'message_receive_loop
                                }
                            }
                            // Do nothing on this packet.
                            // Stream is disconnected.
                            Ok(0) => {
                                logger.send("Stream has been severed while reading", LogLevel::DEBUG);
                                connection_handle_mutator.disconnect();
                            }
                            // RawMessage of invalid size received, close the stream
                            Ok(_) => {
                                logger.send("A message of invalid length has been received", LogLevel::ERROR);
                                connection_handle_mutator.disconnect();
                            }
                            Err(error) => {
                                match error.kind() {
                                    ErrorKind::WouldBlock |
                                    ErrorKind::TimedOut => {
                                        // Check for timeout.
                                        if last_message_timestamp.elapsed() > TCP_TIMEOUT {
                                            logger.send("TCP Socket has timed out.", LogLevel::WARNING);
                                            connection_handle_mutator.disconnect();
                                        } else {
                                            // Drop the lock before sleeping

                                            // Nothing on the line, sleep
                                            sleep(MESSAGE_POLLING_PERIOD);
                                        }
                                    },

                                    // Disconnect immediately on other errors
                                    _ => {
                                        logger.send("An error occurred while reading data", LogLevel::ERROR);
                                        connection_handle_mutator.disconnect();
                                    }
                                }
                            }
                        }
                    }
                    // Currently disconnected, just wait
                    Disconnected => {
                        drop(writeable_stream); // Drop the lock before sleeping.
                        was_connected = false;
                        sleep(MESSAGE_POLLING_PERIOD);
                    },
                    // Handle has been killed, drop thread.
                    Killed => break 'message_receive_loop
                }
            }
        });
        message_tcp_recv_rx
    }

    fn messages_channels(&mut self) -> (Sender<TimedMessage>, Receiver<Message>) {
        let raw_message_recv = self.message_recv_loop();
        let connection_status_recv = self.channels.take_status();

        let connection_handle_mutator = self.get_handle_mutator();

        let (messages_read, handle_message_receiver) = unbounded();
        let (handle_message_sender, messages_to_send) = unbounded::<TimedMessage>();
        let keep_alive_sender = handle_message_sender.clone();

        let logger = self.logger.clone();

        spawn(move || {
            let keep_alive_period = tick(Duration::from_millis(100));

            let mut last_message_sent = Instant::now();
            let mut connection_status = AliveStatus::of(AliveValue::Disconnected);

            'message_receive_loop: loop {
                if is_faulted() { break 'message_receive_loop }

                select_biased!(
                    // Keep track of connection status.
                    recv(connection_status_recv) -> status => {
                        match status {
                            Ok(status) => {
                                connection_status = status;
                                if let Err(_channel_severed) = messages_read.send(status.into()) {
                                    conditional_faulting(&connection_handle_mutator.connection_state,
                                        "Unable to send the status since the channel broke");
                                    break 'message_receive_loop
                                }
                            }
                            Err(_channel_severed) => {
                                conditional_faulting(&connection_handle_mutator.connection_state,
                                    "Unable to transmit packet to the network since the channel broke");
                                break 'message_receive_loop
                            }
                        }
                    }

                    // Handle received message from network.
                    recv(raw_message_recv) -> message => {
                        match message {
                            Ok(message) => {
                                // Drop every message when not connected.
                                if connection_status.is_connected() {
                                    match message {
                                        // Ignore KeepAlive message, they served their purpose
                                        KeepAlive => {},
                                        // not a keep alive, transmit it to the channel
                                        message => {
                                            logger.send(&format!("Received Message: {message:?} at {:?}", Instant::now()), LogLevel::DEBUG);
                                            if let Err(_channel_severed) = messages_read.send(message) {
                                                conditional_faulting(&connection_handle_mutator.connection_state,
                                                    "Unable to send the message since the channel broke");
                                                break 'message_receive_loop
                                            }
                                        }
                                    }
                                }
                            }

                            Err(_channel_severed) => {
                                conditional_faulting(&connection_handle_mutator.connection_state,
                                    "Unable to transmit packet to the network since the channel broke");
                                break 'message_receive_loop
                            }
                        }
                    }

                    // Handle message the client want to send.
                    recv(messages_to_send) -> timed_message => {
                        match timed_message {
                            // If channel is not severed and is_connected.
                            Ok(timed_message) => {
                                if connection_status.is_valid_for(timed_message.timestamp()) {
                                    if let Err(HandleKilled) = Self::message_send(
                                        timed_message.message(),
                                        &mut last_message_sent,
                                        &connection_handle_mutator
                                    ) {
                                        break 'message_receive_loop
                                    }
                                }
                            }

                            Err(_channel_severed) => {
                                conditional_faulting(&connection_handle_mutator.connection_state,
                                    "Unable to transmit packet to the network since the channel broke");
                                break 'message_receive_loop
                            }
                        }
                    },

                    // Send keep alive periodically.
                    recv(keep_alive_period) -> _ => {
                        if connection_status.is_connected() {
                            if let Err(_channel_severed) = keep_alive_sender.send(TimedMessage::of(KeepAlive)) {
                                conditional_faulting(&connection_handle_mutator.connection_state,
                                    "Unable to send KeepAlive packet since the channel broke");
                                break 'message_receive_loop
                            }
                        }
                    }
                );
            }
        });
        (handle_message_sender, handle_message_receiver)
    }

    fn message_send(
        message: Message,
        last_message_sent: &mut Instant,
        connection_handle_mutator: &ConnectionHandleMutator,
    ) -> Result<(), MessageSendError> {
        if message.is_keep_alive() &&
            last_message_sent.elapsed() < SEND_KEEP_ALIVE_PERIOD {
            return Err(KeepAliveTooSoon);
        }

        let mut writeable_stream = connection_handle_mutator.connection_state.write().unwrap();

        match writeable_stream.deref_mut() {
            Connected { stream, .. } => {
                // Likely alive
                if let Ok(MESSAGE_SIZE) = stream.write(&message.encode()) {
                    *last_message_sent = Instant::now();
                    Ok(())
                }
                // Closed for sure
                else {
                    drop(writeable_stream);
                    connection_handle_mutator.logger.send("Stream has been severed while writing", LogLevel::DEBUG);
                    connection_handle_mutator.disconnect();
                    Err(HandleDisconnected)
                }
            }
            Disconnected => {
                // Ignore keepalive messages as they are always and automatically sent, regardless of connection status.
                if !message.is_keep_alive() {
                    connection_handle_mutator.logger.send("Dropped message since stream is not connected", LogLevel::DEBUG)
                }
                Err(HandleDisconnected)
            }
            Killed => { Err(HandleKilled)}
        }
    }
}

enum MessageSendError {
    KeepAliveTooSoon,

    HandleDisconnected,
    HandleKilled,
}