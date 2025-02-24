use crate::connection::connection_channel::{AliveStatusNotifier, ConnectionTransmitters};
use crate::connection::connection_init::init_client::update_main_server_loop;
use crate::connection::constants::{MESSAGE_POLLING_PERIOD, SEND_KEEP_ALIVE_PERIOD, TCP_TIMEOUT};
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::ops::DerefMut;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock, RwLockWriteGuard};
use std::thread::{sleep, spawn};
use std::time::Instant;
use crate::log::log_client::ReliableLogSender;
use crate::log::LogLevel;
use crate::messages::{Message, RawMessage, DEFAULT_MESSAGE, MESSAGE_SIZE, KEEP_ALIVE_MESSAGE};
use crate::program_fault::program_set_to_faulted;

pub(super) enum ConnectionState {
    Connected(TcpStream),
    Disconnected
}

pub struct ConnectionHandle {
    pub(super) connection_state: Arc<RwLock<ConnectionState>>,
    channels: ConnectionTransmitters,
    faulted: Arc<Mutex<bool>>,
    logger: ReliableLogSender,
    killed: Arc<Mutex<bool>>,
}

impl ConnectionHandle {
    fn uninitialized(logger: &ReliableLogSender, faulted: &Arc<Mutex<bool>>, is_server: bool) -> Self {
        let mut connection = Self {
            connection_state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            channels: ConnectionTransmitters::init(),
            faulted: faulted.clone(),
            killed: Arc::new(Mutex::new(false)),
            logger: logger.clone(),
        };

        let msg_tx = connection.encoded_message_write_loop(is_server);
        let msg_rx = connection.encoded_message_recv_loop(is_server);
        connection.channels.populate(msg_tx, msg_rx);

        connection
    }

    pub(super) fn new_server_connection_handler(
        stream: TcpStream,
        logger: ReliableLogSender,
        faulted: &Arc<Mutex<bool>>
    ) -> Self {
        let connection = Self::uninitialized(&logger, faulted, true);

        if let Err(_non_blocking_error) = stream.set_nonblocking(true) {
            *faulted.lock().unwrap() = true;
        }
        *connection.connection_state.write().unwrap() = ConnectionState::Connected(stream);
        connection.channels.alive_status_notifier().is_connected(true);

        connection
    }

    pub fn new_client_connection_handler(logger: ReliableLogSender, faulted: &Arc<Mutex<bool>>) -> Self {
        let mut connection = Self::uninitialized(&logger, faulted, false);

        update_main_server_loop(&mut connection, &logger, faulted);

        connection
    }

    pub fn take_status(&mut self) -> Receiver<bool> {
        self.channels.take_status()
    }
    pub fn take_receiver(&mut self) -> Receiver<RawMessage> {
        self.channels.take_receiver()
    }
    pub fn take_sender(&mut self) -> Sender<RawMessage> {
         self.channels.take_sender()
    }

    pub(super) fn borrow_sender(&self) -> &Sender<RawMessage> {
        self.channels.borrow_sender()
    }

    pub(super) fn borrow_receiver(&self) -> &Receiver<RawMessage> {
        self.channels.borrow_receiver()
    }

    pub(super) fn alive_status_notifier(&self) -> AliveStatusNotifier {
        self.channels.alive_status_notifier()
    }

    pub(super) fn kill(self) {
        Self::disconnect(&self.alive_status_notifier(), &mut self.connection_state.write().unwrap());
        *self.killed.lock().unwrap() = true;
    }

    pub(super) fn disconnect(alive_notifier: &AliveStatusNotifier, writeable_stream: &mut RwLockWriteGuard<ConnectionState>) {
        **writeable_stream = ConnectionState::Disconnected;
        alive_notifier.is_connected(false);
    }

    fn send_keepalive_loop(&mut self, message_tx: &Sender<RawMessage>, is_server: bool) {
        let message_tx = message_tx.clone();
        let faulted = self.faulted.clone();
        let killed = self.killed.clone();

        spawn(move || {
            'keepalive_loop: loop {
                if *faulted.lock().unwrap() { break 'keepalive_loop }
                if *killed.lock().unwrap() { break 'keepalive_loop }

                if let Err(_channel_severed) = message_tx.send(KEEP_ALIVE_MESSAGE) {
                    //TODO MEH
                    if !is_server || !*killed.lock().unwrap() {
                        program_set_to_faulted(&faulted, "Unable to send KeepAlive packet since the channel broke")
                    }
                    break 'keepalive_loop;
                }
                sleep(SEND_KEEP_ALIVE_PERIOD);
            }
        });
    }

    fn encoded_message_recv_loop(&mut self, is_server: bool) -> Receiver<RawMessage> {
        let connection_state = self.connection_state.clone();
        let logger = self.logger.clone();
        let faulted = self.faulted.clone();
        let killed = self.killed.clone();
        let alive_notifier = self.channels.alive_status_notifier();

        let (message_tcp_recv_tx, message_tcp_recv_rx) = channel();
        let mut raw_message_buffer = DEFAULT_MESSAGE;
        let mut was_connected = false;
        let mut last_message_timestamp: Instant = Instant::now();

        spawn(move || {
            'message_receive_loop: loop {
                if *faulted.lock().unwrap() { break 'message_receive_loop }
                if *killed.lock().unwrap() { break 'message_receive_loop }

                let mut writeable_stream     = connection_state.write().unwrap();
                if let ConnectionState::Connected(stream) = writeable_stream.deref_mut() {
                    if !was_connected {
                        was_connected = true;
                        last_message_timestamp = Instant::now();
                    }

                    match stream.read(&mut raw_message_buffer) {
                        // Valid message.
                        Ok(MESSAGE_SIZE) => {
                            logger.send(&format!("Received Messages: {:?}", Message::decode_message(&raw_message_buffer)), LogLevel::DEBUG);
                            last_message_timestamp = Instant::now();
                            // not a keep alive, transmit it to the channel
                            if ! Message::is_keep_alive(&raw_message_buffer) {
                                if let Err(_channel_severed)
                                    = message_tcp_recv_tx.send(raw_message_buffer.clone()) {

                                    // TODO MEH
                                    if !is_server || !*killed.lock().unwrap() {
                                        program_set_to_faulted(&faulted, "Unable to transmit packet from the network since the channel broke");
                                    }

                                    break 'message_receive_loop
                                }
                            }
                        }
                        // Do nothing on this packet.
                        // Stream is disconnected.
                        Ok(0) => {
                            logger.send("Stream has been severed while reading", LogLevel::DEBUG);
                            Self::disconnect(&alive_notifier, &mut writeable_stream);
                        }
                        // RawMessage of invalid size received, close the stream
                        Ok(_) => {
                            logger.send("A message of invalid length has been received", LogLevel::ERROR);
                            Self::disconnect(&alive_notifier, &mut writeable_stream);
                        }
                        Err(error) => {
                            match error.kind() {
                                ErrorKind::WouldBlock |
                                ErrorKind::TimedOut => {
                                    // Check for timeout.
                                    if Instant::now().duration_since(last_message_timestamp) > TCP_TIMEOUT {
                                        logger.send("TCP Socket has timed out.", LogLevel::WARNING);
                                        Self::disconnect(&alive_notifier, &mut writeable_stream);
                                    } else {
                                        drop(writeable_stream);
                                        sleep(MESSAGE_POLLING_PERIOD);
                                    }

                                }, // Nothing on the line, sleep

                                // Disconnect immediately on other errors
                                _ => {
                                    logger.send("An error occurred while reading data", LogLevel::ERROR);
                                    Self::disconnect(&alive_notifier, &mut writeable_stream);
                                }
                            }
                        }
                    }
                }
                // Currently disconnected, just wait
                else {
                    was_connected = false;
                    drop(writeable_stream);
                    sleep(MESSAGE_POLLING_PERIOD);
                }
            }
        });
        message_tcp_recv_rx
    }

    fn encoded_message_write_loop(&mut self, is_server: bool) -> Sender<RawMessage> {
        let connection_state = self.connection_state.clone();
        let logger = self.logger.clone();
        let faulted = self.faulted.clone();
        let killed = self.killed.clone();
        let alive_notifier = self.channels.alive_status_notifier();

        let (message_tcp_write_tx, message_tcp_write_rx) = channel::<RawMessage>();

        let mut last_message_sent = Instant::now();

        Self::send_keepalive_loop(self, &message_tcp_write_tx, is_server);

        spawn(move || {
            'message_receive_loop: loop {
                if *faulted.lock().unwrap() { break 'message_receive_loop }
                if *killed.lock().unwrap() { break 'message_receive_loop }

                match message_tcp_write_rx.recv() {
                    Ok(mut encoded_message) => {
                        // If message is keep alive, send it only if there were no messages for a while
                        if encoded_message[0] == 0u8 &&
                            Instant::now().duration_since(last_message_sent) < SEND_KEEP_ALIVE_PERIOD {
                            continue 'message_receive_loop
                        }

                        let mut writeable_stream = connection_state.write().unwrap();
                        if let ConnectionState::Connected(stream) = writeable_stream.deref_mut() {
                            // Likely alive
                            if let Ok(MESSAGE_SIZE) = stream.write(&mut encoded_message) {
                                last_message_sent = Instant::now();
                            }
                            // Closed for sure
                            else {
                                logger.send("Stream has been severed while writing", LogLevel::DEBUG);
                                Self::disconnect(&alive_notifier, &mut writeable_stream);
                            }
                        } else {
                            // Ignore keepalive messages as they are always and automatically sent, regardless of connection status.
                            if encoded_message[0] != 0u8 {
                                logger.send("Dropped message since stream is not connected", LogLevel::DEBUG)
                            }
                        }
                        drop(writeable_stream);
                    }

                    Err(_channel_severed) => {
                        //TODO MEH
                        if !is_server || !*killed.lock().unwrap() {
                            program_set_to_faulted(&faulted, "Unable to transmit packet to the network since the channel broke");
                        }
                        break 'message_receive_loop
                    }
                }
            }
        });
        message_tcp_write_tx
    }
}