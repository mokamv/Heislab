use crate::connection::connection_channel::{AliveStatus, AliveStatusNotifier, ConnectionTransmitters};
use crate::connection::connection_init::init_client::listen_for_controller_loop;
use crate::connection::constants::{MESSAGE_POLLING_PERIOD, SEND_KEEP_ALIVE_PERIOD, TCP_TIMEOUT};
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, RwLock, RwLockWriteGuard};
use std::thread::{sleep, spawn};
use std::time::Instant;
use crossbeam_channel::{select_biased, tick, unbounded, Receiver, Sender};
use crate::connection::connection_handle::MessageSendError::{HandleDisconnected, HandleKilled, KeepAliveTooSoon};
use crate::log::log_client::ReliableLogSender;
use crate::log::LogLevel;
use crate::messages::{Message, DEFAULT_MESSAGE, MESSAGE_SIZE};
use crate::messages::Message::{Authenticated, KeepAlive};
use crate::program_fault::program_set_to_faulted;

pub(super) enum ConnectionState {
    /// A handle is considered [Connected](ConnectionState::Connected) when it contains an active and alive
    /// [TcpStream].
    Connected {
        /// This field is only used on the client side. Server side must never rely on this value, as it will always be 0.
        /// For a client, this value is the id of the current controller, it is used for reconciliation purpose.
        ///
        /// A client seeing two master controllers will always pick the one of lowest id.
        controller_id: u8,
        stream: TcpStream
    },
    /// A handle is considered [Disconnected](ConnectionState::Disconnected) when it does not contain an active and alive
    /// [TcpStream], but connection is still possible later in time.
    Disconnected,

    /// A handle is considered [Killed](ConnectionState::Killed) when it is definitely disconnected. Associated threads are going
    /// or are already killed.
    Killed
}

#[inline] //TODO Better name?
fn conditional_faulting(
    state: &Arc<RwLock<ConnectionState>>,
    faulted: &Arc<Mutex<bool>>,
    reason: &str
) {
    if let ConnectionState::Killed = state.read().unwrap().deref() {} else { program_set_to_faulted(faulted, reason); }
}

pub struct ConnectionHandle {
    pub(super) connection_state: Arc<RwLock<ConnectionState>>,
    channels: ConnectionTransmitters,
    is_server: bool,
    logger: ReliableLogSender,
    faulted: Arc<Mutex<bool>>,
}

impl ConnectionHandle {
    fn uninitialized(logger: &ReliableLogSender, faulted: &Arc<Mutex<bool>>, is_server: bool) -> Self {
        let mut connection = Self {
            connection_state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            channels: ConnectionTransmitters::init(),
            faulted: faulted.clone(),
            logger: logger.clone(),
            is_server
        };

        let msg_tx = connection.message_send_loop();
        let msg_rx = connection.message_recv_loop();
        connection.channels.populate(msg_tx, msg_rx);

        connection
    }

    pub(super) fn new_temporary_connection_handler(
        stream: TcpStream,
        logger: ReliableLogSender,
        faulted: &Arc<Mutex<bool>>
    ) -> Self {
        let connection = Self::uninitialized(&logger, faulted, true);

        if let Err(_non_blocking_error) = stream.set_nonblocking(true) {
            *faulted.lock().unwrap() = true;
        }
        *connection.connection_state.write().unwrap() = ConnectionState::Connected {
            controller_id: 0, // Server connection handle has always an id of 0.
            stream
        };

        connection
    }

    pub(super) fn new_server_connection_handler(logger: ReliableLogSender, faulted: &Arc<Mutex<bool>>) -> Self {
        Self::uninitialized(&logger, faulted, true)
    }

    pub fn new_client_connection_handler(logger: ReliableLogSender, faulted: &Arc<Mutex<bool>>) -> Self {
        let mut connection = Self::uninitialized(&logger, faulted, false);

        listen_for_controller_loop(&mut connection, &logger, faulted);

        connection
    }

    pub fn take_status(&mut self) -> Receiver<AliveStatus> {
        self.channels.take_status()
    }
    pub fn take_receiver(&mut self) -> Receiver<Message> {
        self.channels.take_receiver()
    }
    pub fn take_sender(&mut self) -> Sender<Message> {
        self.channels.take_sender()
    }

    pub(super) fn borrow_sender(&self) -> &Sender<Message> {
        self.channels.borrow_sender()
    }

    pub(super) fn borrow_receiver(&self) -> &Receiver<Message> {
        self.channels.borrow_receiver()
    }

    pub(super) fn alive_status_notifier(&self) -> AliveStatusNotifier {
        self.channels.get_alive_notifier_instance()
    }

    pub(super) fn notify_status(&self, status: AliveStatus) {
        self.channels.notify_status(status)
    }

    pub(super) fn extract_stream_and_kill(self) -> Option<TcpStream> {
        let stream = match self.connection_state.write().unwrap().deref() {
            ConnectionState::Connected { stream, .. } => {
                match stream.try_clone() {
                    Ok(stream) => Some(stream),
                    Err(_) => None
                }
            }
            ConnectionState::Disconnected => None,

            // Already dead, ignore killing order.
            ConnectionState::Killed => return None
        };

        self.kill();
        stream
    }

    pub(super) fn kill(self) {
        assert_eq!(self.is_server, true, "Cannot kill a client connection handle.");
        *self.connection_state.write().unwrap() = ConnectionState::Killed;
        self.notify_status(AliveStatus::Disconnected); // TODO Maybe change this
    }

    pub(super) fn disconnect(&self) {
        Self::trigger_disconnect(&self.alive_status_notifier(), &mut self.connection_state.write().unwrap())
    }

    pub(super) fn trigger_disconnect(alive_notifier: &AliveStatusNotifier, writeable_stream: &mut RwLockWriteGuard<ConnectionState>) {
        **writeable_stream = ConnectionState::Disconnected;
        alive_notifier.notify_status(AliveStatus::Disconnected);
    }

    pub(super) fn is_connected(&self) -> bool {
        match *self.connection_state.read().unwrap() {
            ConnectionState::Connected { .. } => true,
            ConnectionState::Disconnected | ConnectionState::Killed => false
        }
    }

    fn message_recv_loop(&mut self) -> Receiver<Message> {
        let connection_state = self.connection_state.clone();
        let logger = self.logger.clone();
        let faulted = self.faulted.clone();
        let alive_notifier = self.channels.get_alive_notifier_instance();

        let (message_tcp_recv_tx, message_tcp_recv_rx) = unbounded();
        let mut raw_message_buffer = DEFAULT_MESSAGE;
        let mut was_connected = false;
        let mut last_message_timestamp: Instant = Instant::now();

        spawn(move || {
            'message_receive_loop: loop {
                if *faulted.lock().unwrap() { break 'message_receive_loop }

                let mut writeable_stream = connection_state.write().unwrap();
                match writeable_stream.deref_mut() {
                    ConnectionState::Connected { stream, .. } => {
                        if !was_connected {
                            was_connected = true;
                            last_message_timestamp = Instant::now();
                        }

                        match stream.read(&mut raw_message_buffer) {
                            // Valid message.
                            Ok(MESSAGE_SIZE) => {
                                drop(writeable_stream);

                                logger.send(&format!("Received Messages: {:?}", Message::decode_message(&raw_message_buffer)), LogLevel::DEBUG);
                                last_message_timestamp = Instant::now();

                                match Message::decode_message(&raw_message_buffer) {
                                    // Ignore KeepAlive message, they served their purpose
                                    KeepAlive => {},
                                    // Handle Authenticated message here
                                    Authenticated => {
                                        alive_notifier.notify_status(AliveStatus::ConnectedAndAuthenticated)
                                    }
                                    // not a keep alive, transmit it to the channel
                                    message => {
                                        if let Err(_channel_severed) = message_tcp_recv_tx.send(message) {
                                            conditional_faulting(&connection_state, &faulted, "Unable to transmit packet from the network since the channel broke");
                                            break 'message_receive_loop
                                        }
                                    }
                                }
                            }
                            // Do nothing on this packet.
                            // Stream is disconnected.
                            Ok(0) => {
                                logger.send("Stream has been severed while reading", LogLevel::DEBUG);
                                Self::trigger_disconnect(&alive_notifier, &mut writeable_stream);
                            }
                            // RawMessage of invalid size received, close the stream
                            Ok(_) => {
                                logger.send("A message of invalid length has been received", LogLevel::ERROR);
                                Self::trigger_disconnect(&alive_notifier, &mut writeable_stream);
                            }
                            Err(error) => {
                                match error.kind() {
                                    ErrorKind::WouldBlock |
                                    ErrorKind::TimedOut => {
                                        // Check for timeout.
                                        if last_message_timestamp.elapsed() > TCP_TIMEOUT {
                                            logger.send("TCP Socket has timed out.", LogLevel::WARNING);
                                            Self::trigger_disconnect(&alive_notifier, &mut writeable_stream);
                                        } else {
                                            // Drop the lock before sleeping
                                            drop(writeable_stream);
                                            // Nothing on the line, sleep
                                            sleep(MESSAGE_POLLING_PERIOD);
                                        }
                                    },

                                    // Disconnect immediately on other errors
                                    _ => {
                                        logger.send("An error occurred while reading data", LogLevel::ERROR);
                                        Self::trigger_disconnect(&alive_notifier, &mut writeable_stream);
                                    }
                                }
                            }
                        }
                    }
                    // Currently disconnected, just wait
                    ConnectionState::Disconnected => {
                        was_connected = false;
                        drop(writeable_stream); // Drop the lock before sleeping.
                        sleep(MESSAGE_POLLING_PERIOD);
                    },
                    // Handle has been killed, drop thread.
                    ConnectionState::Killed => break 'message_receive_loop
                }
            }
        });
        message_tcp_recv_rx
    }

    fn message_send_loop(&mut self) -> Sender<Message> {
        let connection_state = self.connection_state.clone();
        let logger = self.logger.clone();
        let faulted = self.faulted.clone();
        let alive_notifier = self.channels.get_alive_notifier_instance();

        let (message_sender, message_receiver) = unbounded();
        let keep_alive_sender = message_sender.clone();

        spawn(move || {
            let mut last_message_sent = Instant::now();

            'message_receive_loop: loop {
                if *faulted.lock().unwrap() { break 'message_receive_loop }

                select_biased!(
                    recv(message_receiver) -> message => {
                        match message {
                            Ok(message) => if let Err(HandleKilled) = Self::message_send(
                                message,
                                &mut last_message_sent,
                                &connection_state,
                                &alive_notifier,
                                &logger
                            ) {
                                break 'message_receive_loop
                            },

                            Err(_channel_severed) => {
                                conditional_faulting(&connection_state, &faulted, "Unable to transmit packet to the network since the channel broke");
                                break 'message_receive_loop
                            }
                        }
                    },
                    recv(tick(SEND_KEEP_ALIVE_PERIOD)) -> _ => {
                        if let Err(_channel_severed) = keep_alive_sender.send(KeepAlive) {
                            conditional_faulting(&connection_state, &faulted, "Unable to send KeepAlive packet since the channel broke");
                            break 'message_receive_loop
                        }
                    }
                );
            }
        });
        message_sender
    }

    fn message_send(
        message: Message,
        last_message_sent: &mut Instant,
        connection_state: &Arc<RwLock<ConnectionState>>,
        alive_status_notifier: &AliveStatusNotifier,
        logger: &ReliableLogSender
    ) -> Result<(), MessageSendError> {
        if message.is_keep_alive() &&
            last_message_sent.elapsed() < SEND_KEEP_ALIVE_PERIOD {
            return Err(KeepAliveTooSoon);
        }

        let mut writeable_stream = connection_state.write().unwrap();
        match writeable_stream.deref_mut() {
            ConnectionState::Connected { stream, .. } => {
                // Likely alive
                if let Ok(MESSAGE_SIZE) = stream.write(&message.encode()) {
                    *last_message_sent = Instant::now();
                    Ok(())
                }
                // Closed for sure
                else {
                    logger.send("Stream has been severed while writing", LogLevel::DEBUG);
                    Self::trigger_disconnect(alive_status_notifier, &mut writeable_stream);
                    Err(HandleDisconnected)
                }
            }
            ConnectionState::Disconnected => {
                // Ignore keepalive messages as they are always and automatically sent, regardless of connection status.
                if !message.is_keep_alive() {
                    logger.send("Dropped message since stream is not connected", LogLevel::DEBUG)
                }
                Err(HandleDisconnected)
            }
            ConnectionState::Killed => { Err(HandleKilled)}
        }
    }
}

enum MessageSendError {
    KeepAliveTooSoon,

    HandleDisconnected,
    HandleKilled,
}