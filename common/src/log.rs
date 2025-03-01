use crate::log::log_message::LogMessageError;
use std::cmp::Ordering;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

const CONNECTION_RETRY: Duration = Duration::from_secs(5);
const DEAD_LOGGER_PURGE_PERIOD: Duration = Duration::from_secs(120);

const LOG_SERVER_TCP_PORT: u16 = 8000;
const LOG_SERVER_TCP_ADDRESS: SocketAddrV4 =
    SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), LOG_SERVER_TCP_PORT);

/// Provides a level to attach to a log message or to a log server so that
/// messages can be filtered.
///
/// Per usual with logging levels, [Debug](LogLevel::DEBUG) is for a server, equivalent to not filtering any log message,
/// and messages tagged with this level will only be displayed by log server of the same level.
/// A log server tagged with [Error](LogLevel::ERROR) will only display messages also tagged with [Error](LogLevel::ERROR)
#[derive(Ord, Eq, PartialOrd, PartialEq, Copy, Clone, Debug)]
pub enum LogLevel {
    DEBUG = 0,
    INFO = 1,
    WARNING = 2,
    ERROR = 3
}

impl TryFrom<isize> for LogLevel {
    type Error = LogMessageError;

    /// Convenience function to convert an [isize] to a [LogLevel](LogLevel)
    fn try_from(v: isize) -> Result<Self, Self::Error> {
        match v {
            x if x == LogLevel::DEBUG as isize => Ok(LogLevel::DEBUG),
            x if x == LogLevel::INFO as isize => Ok(LogLevel::INFO),
            x if x == LogLevel::WARNING as isize => Ok(LogLevel::WARNING),
            x if x == LogLevel::ERROR as isize => Ok(LogLevel::ERROR),
            _ => Err(Self::Error::BadLogLevel),
        }
    }
}

impl LogLevel {
    /// Convenience function to indicate if a log server will filter out a log message.
    fn can_log(&self, message_log_level: &Self) -> bool {
        match self.cmp(message_log_level) {
            Ordering::Equal | Ordering::Less => true,
            Ordering::Greater => false
        }
    }
}

/// Private module used to represent and convert structure called LogMessage, which are
/// the combination of a [message](String) and a [LogLevel].
///
/// This module is used internally to ease the conversion to and from raw form, and also to facilitate filtering.
mod log_message {
    use crate::log::LogLevel;

    pub const MAX_MESSAGE_LENGTH: usize = 4096;

    pub type LogBodyPart = [u8; 1024];
    pub type LogHeader = [u8; size_of::<isize>() + size_of::<usize>()];

    /// Generate an unset, owned raw byte array corresponding to [LogHeader] specs.
    pub fn empty_log_header() -> LogHeader {
        [0u8; size_of::<LogHeader>()]
    }

    #[derive(Debug)]
pub enum LogMessageError {
        BadLogLevel,
        MessageTooLarge
    }

    /// Convert a [LogHeader], which is basically a raw byte array, to a log level and a message size.
    ///
    /// This function is the counterpart of [encode_header]
    pub fn decode_header(header: LogHeader) -> Result<(LogLevel, usize), LogMessageError> {
        let (log_level_bytes, message_size_bytes) = header.split_at(size_of::<isize>());

        let message_size = usize::from_be_bytes(message_size_bytes.try_into().unwrap());
        if message_size > MAX_MESSAGE_LENGTH { return Err(LogMessageError::MessageTooLarge) }

        Ok((
            LogLevel::try_from(isize::from_be_bytes(log_level_bytes.try_into().unwrap()))?,
            message_size
        ))
    }

    /// Convert a [LogLevel] and a message size to a raw byte array representation, also called [LogHeader]
    ///
    /// This function is the counterpart of [decode_header]
    pub fn encode_header(log_level: LogLevel, message_size: usize) -> Result<LogHeader, LogMessageError> {
        if message_size > MAX_MESSAGE_LENGTH { return Err(LogMessageError::MessageTooLarge) }

        let mut header = empty_log_header();

        header[..size_of::<isize>()].copy_from_slice(&(log_level as isize).to_be_bytes());
        header[size_of::<isize>()..].copy_from_slice(&message_size.to_be_bytes());

        Ok(header)
    }

    /// Actual data structure to represent log messages inside the whole logging module
    pub struct LogMessage {
        pub log_level: LogLevel,
        pub message: String
    }

    impl LogMessage {
        /// Convenience function to convert a [LogMessage] to its raw byte representation.
        ///
        /// Note that there is no counterpart function to this one since the header is first decoded to obtain the
        /// message size and level before actually getting the message body.
        pub fn as_bytes(&self) -> Result<Box<[u8]>, LogMessageError> {
            let mut msg_bytes = Vec::with_capacity(size_of::<LogHeader>() + self.message.len());

            msg_bytes.extend_from_slice(&encode_header(self.log_level, self.message.len())?);
            msg_bytes.extend(self.message.as_bytes());

            Ok(msg_bytes.into_boxed_slice())
        }
    }
}

/// This module is responsible for providing the log server to any application wanting to implement it.
///
/// The log server can be used as a middleware by using [act_as_middleware_logger]. This allows users to
/// implement their own logic with logs without having to implement the log collection and client part.
///
/// A utility function - [act_as_primary_logger] - is also provided, as an in-house middleware, that only redirect all logs to stdout
pub mod log_server {
    use crate::log::log_message::{decode_header, empty_log_header, LogBodyPart, LogHeader, LogMessage, LogMessageError};
    use crate::log::{LogLevel, LOG_SERVER_TCP_ADDRESS};
    use std::cmp::min;
    use std::io::{Error, ErrorKind, Read};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc::{channel, Receiver, SendError, Sender};
    use std::thread::spawn;

    /// List of potential errors the log server could encounter.
    #[derive(Debug)]
    pub enum LogServerError {
        /// The socket address is already bound or non-bindable, it probably means that another log server is running
        /// or that another software is bound to this very address.
        CantBind,

        /// The server channel is severed, this is unrecoverable, and should be treated has a fatal error leading to a [ServerDeadChannel](LogServerError::ServerDeadChannel)
        /// error.
        ServerDeadChannel,

        /// The server has faced an error that crashed it, there is nothing that can be done other that relaunching a log server.
        ServerTerminated,

        /// A client has been disconnected. It can be because of a network problem or just because the client has stopped normally.
        /// This error is not fatal in any way, and it shouldn't be treated has such.
        ClientDisconnect,
        /// A client has sent wrongly formatted or a too long message. The main reason of this error is a version mismatch between server and client
        /// A network error might be, in some very rare case, the reason of this error.
        ClientSendBadData(LogMessageError),
        /// A client has timed out will a message was being received, that is when the header is already received but not the entirety of the content.
        ClientMsgReadTimeout,
    }

    impl From<Error> for LogServerError {
        /// Convert I/O errors from sockets and streams to custom errors used by the module.
        fn from(value: Error) -> Self {
            match value.kind() {
                ErrorKind::AddrInUse | ErrorKind::AddrNotAvailable => {
                    LogServerError::CantBind
                },
                ErrorKind::ConnectionAborted
                | ErrorKind::ConnectionReset
                | ErrorKind::NotConnected
                | ErrorKind::TimedOut
                | ErrorKind::UnexpectedEof
                | ErrorKind::HostUnreachable => LogServerError::ClientDisconnect,

                other => panic!("Untreated error {}, please implement", other)
            }
        }
    }

    /// Convert messages related errors to larger, server related errors.
    impl From<LogMessageError> for LogServerError {
        fn from(value: LogMessageError) -> Self {
            LogServerError::ClientSendBadData(value)
        }
    }

    impl<T> From<SendError<T>> for LogServerError {
        fn from(_: SendError<T>) -> Self {
            LogServerError::ServerDeadChannel
        }
    }

    /// Convenience function acting as a log middleware, and redirecting all non-filtered messages to stdout
    pub fn act_as_primary_logger(log_level: LogLevel) -> Result<(), LogServerError> {
        let logger = act_as_middleware_logger(log_level)?;

        loop {
            match logger.recv() {
                Ok(log_message) => {
                    print!("[{:?}]{}", log_message.log_level, log_message.message)
                }
                Err(_logger_error) => {
                    break
                }
            }
        };

        // Since the receiver is broken, we cannot use it as a way to log the error.
        println!("Logger is broken");
        Err(LogServerError::ServerTerminated)
    }

    /// Main function of the log server module, provides a channel of all collected logs, already filtered.
    pub fn act_as_middleware_logger(log_level: LogLevel) -> Result<Receiver<LogMessage>, LogServerError> {
        let (logging_tx, logging_rx)
            = channel();

        // EPOLL, NON BLOCKING + EVENTFD TO NOTIFY (Rust is so hard to work with in this way...)
        // Why should I use posix because there are no std for epoll.....
        let socket = TcpListener::bind(LOG_SERVER_TCP_ADDRESS)?;

        // This thread listen to incoming client connection.
        spawn(move || {
            for conn in socket.incoming() {
                if let Ok(mut conn) = conn {
                    let logging_tx = logging_tx.clone();

                    spawn(move || {
                        if let Err(client) = handle_connected_logger(&mut conn, &logging_tx, log_level) {
                            match client {
                                LogServerError::ClientDisconnect
                                | LogServerError::ClientSendBadData(_)
                                | LogServerError::ClientMsgReadTimeout => log_client_disconnect(conn, logging_tx),

                                // PROBABLY BY IMPLEMENTING EPOLL. (MEH)
                                LogServerError::ServerDeadChannel => todo!("Need to kill the whole process"),

                                _ => panic!("Cannot happen")
                            }
                        };
                    });
                }
            };

            if let Err(_) = logging_tx.send(LogMessage {
                log_level: LogLevel::ERROR,
                message: "Logger handler has been lost".to_string(),
            }) {
                println!("[ERROR] Logger handler and Logger channel has both been lost")
            }
            drop(logging_tx);
        });

        Ok(logging_rx)
    }

    fn handle_connected_logger(
        client_tcp_stream: &mut TcpStream,
        logging_tx: &Sender<LogMessage>,
        logger_level: LogLevel
    ) -> Result<(), LogServerError> {
        let mut header_buffer: LogHeader = empty_log_header();
        let mut body_buffer: LogBodyPart = [0u8; 1024];

        loop {
            // Receive the header from network.
            client_tcp_stream.read_exact(&mut header_buffer)?;

            // Try to decode the header.
            let (message_log_level, message_size) = decode_header(header_buffer)?;

            // Handle the reception of the message body.
            match handle_message_recv(client_tcp_stream, &mut body_buffer, &logger_level, message_log_level, message_size)? {
                // Message was received but is not loggable at this level
                None => {}

                // If message is valid and loggable, send it through the sender
                Some(message_body) => {
                    logging_tx.send(
                        LogMessage {
                            log_level: message_log_level,
                            message: message_body,
                        }
                    )?
                }
            };
        }
    }


    fn handle_message_recv(
        conn: &mut TcpStream,
        body_buffer: &mut LogBodyPart,
        logger_level: &LogLevel,
        message_log: LogLevel,
        message_size: usize
    ) -> Result<Option<String>, LogServerError> {
        let buffer_size = size_of::<LogBodyPart>();

        let mut remaining = message_size;
        let mut message = Vec::with_capacity(message_size);

        // TODO TIMEOUT ?
        loop {
            let readable = min(buffer_size, remaining);
            if readable == 0 { break };

            conn.read_exact(&mut body_buffer[..readable])?;

            message.extend_from_slice(&body_buffer[..readable]);
            remaining -= readable;
        }

        Ok(
            if !logger_level.can_log(&message_log) { None } else { Some(String::from_utf8(message).unwrap()) }
        )
    }

    /// Convenience function to signal the lost of connection with a logging client.
    fn log_client_disconnect(conn: TcpStream, logging_tx: Sender<LogMessage>) {
        if let Err(_channel_error) = logging_tx.send(
            LogMessage {
                log_level: LogLevel::INFO,
                message: format!("[LOGGER] Lost connection to {:?}\n", conn.peer_addr().unwrap())
            }
        ) {
            println!("Unable to communicate logger client failure to logger server")
        };
    }
}

pub mod log_client {
    use crate::log::log_message::LogMessage;
    use crate::log::{LogLevel, CONNECTION_RETRY, DEAD_LOGGER_PURGE_PERIOD, LOG_SERVER_TCP_ADDRESS};
    use std::collections::VecDeque;
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Arc, Mutex, Weak};
    use std::thread::{sleep, spawn, JoinHandle};

    pub struct Logger {
        logger_inst: Arc<LoggerImpl>
    }

    impl Logger {
        pub fn init(faulted: &Arc<Mutex<bool>>) -> Self {
            Self {
                logger_inst: Arc::new(LoggerImpl::init(faulted)),
            }
        }

        pub fn get_sender(&mut self, prefix: String) -> ReliableLogSender {
            let new_sender = Arc::new(Mutex::new(Some(
                self.logger_inst.original_sender.clone()
            )));

            self.logger_inst.emitted_senders.lock().unwrap().push(Arc::downgrade(&new_sender));

            ReliableLogSender {
                wrapped_sender: new_sender,
                associated_logger: Arc::downgrade(&self.logger_inst),
                prefix
            }
        }

        pub fn send_once(&mut self, message: String, level: LogLevel) {
            if let Err(_send_error) = self.logger_inst.original_sender.send(LogMessage {
                log_level: level,
                message,
            }) {
                println!("Logger is dead");
            }
        }

        pub fn wait_for_logger_termination(self) {
            let logger_impl = Arc::into_inner(self.logger_inst).unwrap();

            let emitted_senders = logger_impl.emitted_senders.lock().unwrap();

            for logger in emitted_senders.iter() {
                if let Some(logger) = logger.upgrade() {
                    *logger.lock().unwrap() = None;
                }
            }
            drop(emitted_senders);


            drop(logger_impl.original_sender);
            logger_impl.sync_thread.join().expect("TODO: panic message");
        }
    }

    struct LoggerImpl {
        original_sender: Sender<LogMessage>,
        connect_socket: Arc<Mutex<Option<TcpStream>>>,
        sync_thread: JoinHandle<()>,
        emitted_senders: Arc<Mutex<Vec<Weak<Mutex<Option<Sender<LogMessage>>>>>>>
    }

    impl LoggerImpl {
        fn init(faulted: &Arc<Mutex<bool>>) -> Self {
            let logger = Self::with_logging_loop(&faulted);

            logger.socket_liveliness_loop(&faulted);
            logger.purge_dead_loggers_loop(&faulted);

            logger
        }

        fn with_logging_loop(faulted: &Arc<Mutex<bool>>) -> Self {
            let (logging_tx, logging_rx) = channel::<LogMessage>();
            let connect_socket: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));

            let mut log_queue: VecDeque<LogMessage> = VecDeque::with_capacity(128);
            let faulted = faulted.clone();
            let sync_socket = connect_socket.clone();
            let sync_thread = spawn(move || {
                'socket_listener: loop {
                    let is_buffer_empty = Self::try_send_from_buffer(&mut log_queue, &sync_socket, &faulted);

                    match logging_rx.recv() {
                        Ok(mut message_to_log) => {
                            if !message_to_log.message.ends_with("\n") {
                                message_to_log.message.push('\n');
                            }

                            if is_buffer_empty {
                                Self::try_send_channel_message(&mut log_queue, message_to_log, &sync_socket, &faulted)
                            } else {
                                Self::store_channel_message(&mut log_queue, message_to_log);
                            }
                        },
                        Err(_channel_error) => { break 'socket_listener }
                    }
                }

                Self::try_send_from_buffer(&mut log_queue, &sync_socket, &Arc::new(Mutex::new(false)));
            });

            Self {
                original_sender: logging_tx,
                sync_thread,
                connect_socket,
                emitted_senders: Arc::new(Mutex::new(vec![]))
            }
        }

        fn socket_liveliness_loop(&self, faulted: &Arc<Mutex<bool>>) {
            let faulted = faulted.clone();
            let connect_socket = self.connect_socket.clone();

            spawn(move || {
                loop {
                    if *faulted.lock().unwrap() { break };

                    let mut socket_lock = connect_socket.lock().unwrap();
                    if socket_lock.is_none() {
                        match TcpStream::connect(LOG_SERVER_TCP_ADDRESS) {
                            Ok(socket) => {
                                *socket_lock = Some(socket);
                            }
                            Err(_socket_error) => {}
                        }
                    }
                    drop(socket_lock);

                    sleep(CONNECTION_RETRY);
                }
            });
        }

        fn purge_dead_loggers_loop(&self, faulted: &Arc<Mutex<bool>>) {
            let faulted = faulted.clone();
            let emitted_senders = self.emitted_senders.clone();

            spawn(move || {
                loop {
                    if *faulted.lock().unwrap() { break };

                    let mut emitted_senders = emitted_senders.lock().unwrap();
                    emitted_senders.retain(|logger| {
                        if let None = logger.upgrade() {
                            false
                        } else {
                            true
                        }
                    });

                    drop(emitted_senders);

                    sleep(DEAD_LOGGER_PURGE_PERIOD);
                }
            });
        }

        fn try_send_from_buffer(
            log_queue: &mut VecDeque<LogMessage>,
            socket: &Arc<Mutex<Option<TcpStream>>>,
            faulted: &Arc<Mutex<bool>>
        ) -> bool {
            // Do nothing when `log_queue` is empty
            if log_queue.is_empty() { return true; }

            let mut socket_lock = socket.lock().unwrap();

            if socket_lock.is_none() { return false; }

            let mut socket = socket_lock.take().unwrap();

            while !log_queue.is_empty() {
                if *faulted.lock().unwrap() { break };

                match socket.write(&log_queue.pop_front().unwrap().as_bytes().expect("TODO")) { //TODO ERROR HANDLING
                    Ok(0) | Err(_) => { return false },
                    Ok(_) => {}
                };
            }

            *socket_lock = Some(socket);
            true
        }

        fn try_send_channel_message(
            log_queue: &mut VecDeque<LogMessage>,
            msg_to_log: LogMessage,
            socket: &Arc<Mutex<Option<TcpStream>>>,
            faulted: &Arc<Mutex<bool>>
        ) {
            if *faulted.lock().unwrap() {
                Self::store_channel_message(log_queue, msg_to_log);
                return;
            }

            let mut socket_option = socket.lock().unwrap();

            match socket_option.take() {
                None => log_queue.push_back(msg_to_log),
                Some(mut socket) => {
                    match socket.write(&msg_to_log.as_bytes().expect("TODO")) { //TODO ERROR HANDLING
                        Ok(0) | Err(_) => {
                            drop(socket);
                            log_queue.push_back(msg_to_log);
                        },
                        Ok(_) => *socket_option = Some(socket)
                    }
                }
            }

            drop(socket_option);
            print!("");
        }

        fn store_channel_message(log_queue: &mut VecDeque<LogMessage>, msg_to_log: LogMessage) {
            log_queue.push_back(msg_to_log);
        }
    }

    pub struct ReliableLogSender {
        wrapped_sender: Arc<Mutex<Option<Sender<LogMessage>>>>,
        associated_logger: Weak<LoggerImpl>,
        prefix: String
    }

    impl ReliableLogSender {
        pub fn send(&self, value: &str, level: LogLevel) {
            match &*self.wrapped_sender.lock().unwrap() {
                None => {}
                Some(wrapped_sender) => {
                    if let Err(_send_error) = wrapped_sender.send(
                        LogMessage {
                            log_level: level,
                            message: String::from(&self.prefix) + " " + &value
                        }
                    ) {
                        println!("LogSender instance is dead");
                    }
                }
            }
        }

        pub fn clone_with_new_prefix(&self, prefix: String) -> Self {
            let logger = match self.associated_logger.upgrade() {
                None => panic!("Logger is dead"),
                Some(logger) => logger
            };

            Logger { logger_inst: logger }.get_sender(prefix)
        }
    }

    impl Clone for ReliableLogSender {
        fn clone(&self) -> Self {
            self.clone_with_new_prefix(self.prefix.clone())
        }
    }
}