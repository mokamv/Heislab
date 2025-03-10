use crate::log_message::LogMessageError;
use std::cmp::Ordering;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

const CONNECTION_RETRY: Duration = Duration::from_secs(5);
const SELECT_TIMEOUT: Duration = Duration::from_millis(50);
const URGENT_WRITE_TIMEOUT: Duration = Duration::from_millis(50);
const WRITE_TIMEOUT: Duration = Duration::from_millis(50);

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
    use crate::LogLevel;
    use std::mem::size_of;

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
    use crate::log_message::{decode_header, empty_log_header, LogBodyPart, LogHeader, LogMessage, LogMessageError};
    use crate::{LogLevel, LOG_SERVER_TCP_ADDRESS};
    use crossbeam_channel::{unbounded, Receiver, SendError, Sender};
    use std::cmp::min;
    use std::io::{Error, ErrorKind, Read};
    use std::mem::size_of;
    use std::net::{TcpListener, TcpStream};
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
            = unbounded();

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
    use crate::log_message::LogMessage;
    use crate::{LogLevel, CONNECTION_RETRY, LOG_SERVER_TCP_ADDRESS, SELECT_TIMEOUT, URGENT_WRITE_TIMEOUT, WRITE_TIMEOUT};
    use crossbeam_channel::{select, tick, unbounded, Receiver, Sender};
    use faulted::{is_faulted, set_to_faulted};
    use std::collections::VecDeque;
    use std::io::Write;
    use std::net::TcpStream;
    use std::ops::Deref;
    use std::sync::Mutex;
    use std::thread::{spawn, JoinHandle};
    use std::time::Instant;

    static LOGGER: Logger = Logger::uninit();

    pub struct Logger {
        logger_impl: Mutex<Option<LoggerImpl>>
    }

    impl Logger {
        const fn uninit() -> Self {
            Self {
                logger_impl: Mutex::new(None),
            }
        }

        pub fn init_logger() {
            let mut logger_impl = LOGGER.logger_impl.lock().unwrap();
            match *logger_impl {
                None => *logger_impl = Some(LoggerImpl::init()),
                Some(_) => set_to_faulted("Logger is already initialized")
            }
        }

        fn dealloc_logger() {
            let mut logger_impl = LOGGER.logger_impl.lock().unwrap();
            match *logger_impl {
                None => set_to_faulted("Logger is not initialized"),
                Some(_) => *logger_impl = None
            }
        }

        pub fn get_sender(prefix: String) -> ReliableLogSender {
            let logger_impl = LOGGER.logger_impl.lock().unwrap();
            match logger_impl.deref() {
                None => {
                    set_to_faulted("Logger is not initialized");
                    ReliableLogSender::no_op_sender()
                }
                Some(logger_impl) => {
                    let wrapped_sender = logger_impl.original_sender.clone();

                    ReliableLogSender {
                        wrapped_sender,
                        prefix
                    }
                }
            }
        }

        pub fn send_once(message: String, level: LogLevel) {
            let logger_impl = LOGGER.logger_impl.lock().unwrap();
            match logger_impl.deref() {
                None => set_to_faulted("Logger is not initialized"),
                Some(logger_impl) => {
                    if let Err(_send_error) = logger_impl.original_sender.send(LogMessage {
                        log_level: level,
                        message,
                    }) {
                        set_to_faulted("Logger channel is severed");
                    }
                }
            }
        }

        pub fn terminate_logging() {
            let mut logger_impl = LOGGER.logger_impl.lock().unwrap();
            match logger_impl.take() {
                None => set_to_faulted("Logger is not initialized"),
                Some(logger_impl) => {
                    match logger_impl.shutdown_channel.send(()) {
                        Ok(_) => {}
                        Err(_) => set_to_faulted("Logging cannot be shutdown properly because the channel is closed.")
                    }
                    match logger_impl.sync_thread.join() {
                        Ok(_) => {}
                        Err(_) => set_to_faulted("Logging thread has panicked during its execution.")
                    };
                }
            }
            Self::dealloc_logger()
        }
    }

    struct LoggerImpl {
        original_sender: Sender<LogMessage>,
        shutdown_channel: Sender<()>,
        sync_thread: JoinHandle<()>,
    }

    impl LoggerImpl {
        fn init() -> Self {
            let (logging_tx, logging_rx) = unbounded::<LogMessage>();
            let (shutdown_tx, shutdown_rx) = unbounded();

            let mut tcp_stream: Option<TcpStream> = None;

            let connection_liveliness_tick: Receiver<Instant> = tick(CONNECTION_RETRY);

            let mut log_queue: VecDeque<LogMessage> = VecDeque::with_capacity(128);

            let sync_thread = spawn(move || {
                'socket_listener: loop {
                    if is_faulted() { break 'socket_listener }
                    select! {
                        recv(shutdown_rx) -> _ => break 'socket_listener,
                        recv(connection_liveliness_tick) -> _ => Self::socket_liveliness(&mut tcp_stream),
                        recv(logging_rx) -> message => match message {
                            Ok(message) => Self::try_send(message, &mut log_queue, &mut tcp_stream),
                            Err(_) => break 'socket_listener
                        },
                        default(SELECT_TIMEOUT) => {
                            Self::try_send_from_buffer(&mut log_queue, &mut tcp_stream, false);
                        }
                    }
                }

                Self::try_send_from_buffer(&mut log_queue, &tcp_stream, true);
            });

            Self {
                original_sender: logging_tx,
                shutdown_channel: shutdown_tx,
                sync_thread,
            }
        }


        fn try_send(
            mut message_to_log: LogMessage,
            log_queue: &mut VecDeque<LogMessage>,
            tcp_stream: &Option<TcpStream>
        ) {
            let is_buffer_empty = Self::try_send_from_buffer(log_queue, tcp_stream, false);

            if !message_to_log.message.ends_with("\n") {
                message_to_log.message.push('\n');
            }

            if is_buffer_empty {
                Self::try_send_channel_message(log_queue, message_to_log, tcp_stream)
            } else {
                Self::store_channel_message(log_queue, message_to_log);
            }
        }


        fn socket_liveliness(tcp_stream: &mut Option<TcpStream>) {
            if tcp_stream.is_none() {
                match TcpStream::connect(LOG_SERVER_TCP_ADDRESS) {
                    Ok(socket) => {
                        if socket.set_write_timeout(Some(WRITE_TIMEOUT)).is_err() {
                            set_to_faulted("Cannot change timeout period")
                        }
                        *tcp_stream = Some(socket);
                    }
                    Err(_socket_error) => {}
                }
            }
        }

        fn try_send_from_buffer(
            log_queue: &mut VecDeque<LogMessage>,
            tcp_stream: &Option<TcpStream>,
            urgent: bool
        ) -> bool {
            // Do nothing when `log_queue` is empty
            if log_queue.is_empty() { return true; }

            match tcp_stream.as_ref() {
                None => false,
                Some(mut tcp_stream) => {
                    if tcp_stream.set_write_timeout(Some(
                        if urgent {URGENT_WRITE_TIMEOUT} else {WRITE_TIMEOUT}
                    )).is_err() {
                        set_to_faulted("Cannot change timeout period")
                    }

                    while !log_queue.is_empty() {
                        match tcp_stream.write(&log_queue.pop_front().unwrap().as_bytes().unwrap()) {
                            Ok(0) | Err(_) => { return false },
                            Ok(_) => {}
                        };
                    }
                    true
                }
            }
        }

        fn try_send_channel_message(
            log_queue: &mut VecDeque<LogMessage>,
            msg_to_log: LogMessage,
            tcp_stream: &Option<TcpStream>
        ) {
            if is_faulted() {
                Self::store_channel_message(log_queue, msg_to_log);
                return;
            }

            match tcp_stream.as_ref() {
                None => Self::store_channel_message(log_queue, msg_to_log),
                Some(mut socket) => {
                    match socket.write(&msg_to_log.as_bytes().expect("TODO")) { //TODO ERROR HANDLING
                        Ok(0) | Err(_) => {
                            Self::store_channel_message(log_queue, msg_to_log);
                        },
                        Ok(_) => {}
                    }
                }
            }
        }

        #[inline]
        fn store_channel_message(log_queue: &mut VecDeque<LogMessage>, msg_to_log: LogMessage) {
            log_queue.push_back(msg_to_log);
        }
    }

    pub struct ReliableLogSender {
        wrapped_sender: Sender<LogMessage>,
        prefix: String
    }

    impl ReliableLogSender {
        pub fn send(&self, value: &str, level: LogLevel) {
            if let Err(_send_error) = self.wrapped_sender.send(
                LogMessage {
                    log_level: level,
                    message: String::from(&self.prefix) + " " + &value
                }
            ) {
                println!("LogSender instance is dead");
            }
        }

        fn no_op_sender() -> Self {
            let (no_op_sender, _) = unbounded();

            Self {
                wrapped_sender: no_op_sender,
                prefix: String::new(),
            }
        }
    }

    impl Clone for ReliableLogSender {
        fn clone(&self) -> Self {
            Logger::get_sender(self.prefix.clone())
        }
    }
}