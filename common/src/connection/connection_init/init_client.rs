use crate::connection::connection_channel::{AliveStatus, AliveStatusNotifier};
use crate::connection::connection_handle::{ConnectionHandle, ConnectionState};
use crate::connection::controller_state::ControllerState;
use crate::connection::constants::ip_addresses::{CLIENT_UDP_LISTEN_ADDR, SERVER_UDP_BIND_ADDR};
use crate::connection::constants::{
    BIND_MAX_RETRY, BIND_RETRY_PERIOD, BROADCAST_PERIOD, UDP_BROADCAST_TIMEOUT,
};
use crate::connection::unix_socket::udp_socket_sharing_port;
use crate::log::log_client::ReliableLogSender;
use crate::log::LogLevel;
use crate::messages::{Message, DEFAULT_MESSAGE, MESSAGE_SIZE};
use std::cmp::Ordering;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{sleep, spawn};
use std::time::Instant;

pub fn listen_for_controller_loop(
    connection_handler: &mut ConnectionHandle,
    logger: &ReliableLogSender,
    faulted: &Arc<Mutex<bool>>,
) {
    let logger = logger.clone();
    let faulted = faulted.clone();
    let connection_state = connection_handler.connection_state.clone();
    let alive_notifier = connection_handler.alive_status_notifier();

    let mut bind_retry_count = 0u32;
    spawn(move || {
        'unbound: while bind_retry_count < BIND_MAX_RETRY {
            if *faulted.lock().unwrap() {
                break 'unbound;
            }

            match udp_socket_sharing_port(CLIENT_UDP_LISTEN_ADDR) {
                Ok(udp_socket) => {
                    if let Err(_cannot_set_timeout) =
                        udp_socket.set_read_timeout(Some(BROADCAST_PERIOD))
                    {
                        println!("Timeout duration cannot be 0");
                        break 'unbound;
                    }

                    bind_retry_count = 0;

                    println!("Successfully bounded to UDP Socket. Starting to receive frames.");

                    let mut last_valid_message = Instant::now();

                    'while_bound: loop {
                        if *faulted.lock().unwrap() {
                            break 'unbound;
                        };
                        if !try_handle_packet(
                            &udp_socket,
                            &connection_state,
                            &alive_notifier,
                            &logger,
                            &mut last_valid_message,
                        ) {
                            break 'while_bound;
                        };

                        if let ConnectionState::Connected { .. } =
                            connection_state.read().unwrap().deref()
                        {
                            if Instant::now().duration_since(last_valid_message)
                                > UDP_BROADCAST_TIMEOUT
                            {
                                logger.send("No UDP frames were received in some times. Considering the connection as timed out", LogLevel::WARNING);
                                ConnectionHandle::trigger_disconnect(
                                    &alive_notifier,
                                    &mut connection_state.write().unwrap(),
                                );
                            }
                        }
                    }

                    // Always disconnect if unbound from udp.
                    ConnectionHandle::trigger_disconnect(
                        &alive_notifier,
                        &mut connection_state.write().unwrap(),
                    );
                }
                Err(_cannot_bind_socket) => {
                    bind_retry_count += 1;
                }
            };
            sleep(BIND_RETRY_PERIOD);
        }

        logger.send("Cannot bind to UDP", LogLevel::ERROR);
        *faulted.lock().unwrap() = true;
    });
}

fn try_handle_packet(
    udp_socket: &UdpSocket,
    connection_state: &Arc<RwLock<ConnectionState>>,
    alive_notifier: &AliveStatusNotifier,
    logger: &ReliableLogSender,
    last_valid_message: &mut Instant,
) -> bool {
    let mut received_message = DEFAULT_MESSAGE;

    match udp_socket.recv_from(&mut received_message) {
        // Check for udp frames coming from the server
        Ok((MESSAGE_SIZE, SERVER_UDP_BIND_ADDR)) => {
            if let Message::SocketAddress { id, state, address } =
                Message::decode_message(&received_message)
            {
                if state == ControllerState::MASTER {
                    *last_valid_message = Instant::now();
                    try_to_connect(connection_state, id, alive_notifier, address, logger);
                }
                true
            } else {
                logger.send(
                    "The server sent an non address packet over the udp broadcast address",
                    LogLevel::ERROR,
                );
                false
            }
        }

        Ok((_, SERVER_UDP_BIND_ADDR)) => {
            logger.send("Message of incorrect size received", LogLevel::ERROR);
            false
        }

        // Discarding every non-server packets
        Ok((_, _)) => true,

        // Explicit error
        Err(error) => {
            match error.kind() {
                // Here, blocking is equivalent to timing out.
                ErrorKind::WouldBlock | ErrorKind::TimedOut => true,

                // All other kinds of errors.
                _ => false,
            }
        }
    }
}

fn try_to_connect(
    connection_state: &Arc<RwLock<ConnectionState>>,
    controller_id: u8,
    alive_notifier: &AliveStatusNotifier,
    address: SocketAddr,
    logger: &ReliableLogSender,
) {
    // Check if the connection is already established
    if let ConnectionState::Connected { stream, controller_id: current_controller_id } = connection_state.read().unwrap().deref() {
        if let Ok(current_socket_addr) = stream.peer_addr() {
            if current_socket_addr == address {
                return;
            }

            match current_controller_id.cmp(&controller_id) {
                Ordering::Less => {}
                | Ordering::Equal => return,
                Ordering::Greater => {} // In this case, the policy is to stick with the lowest id controller.
            }
        }
    }

    // If current connection is in fact dead, try to establish new connection
    ConnectionHandle::trigger_disconnect(alive_notifier, &mut connection_state.write().unwrap());
    *connection_state.write().unwrap() = try_to_get_socket(address, controller_id, alive_notifier);

    match connection_state.read().unwrap().deref() {
        ConnectionState::Connected { stream, controller_id } => logger.send(
            &format!("Connected to controller {} at {:?}", controller_id, stream.peer_addr()),
            LogLevel::INFO,
        ),
        _ => logger.send("Disconnected", LogLevel::INFO),
    }
}

fn try_to_get_socket(address: SocketAddr, controller_id: u8, alive_notifier: &AliveStatusNotifier) -> ConnectionState {
    match TcpStream::connect(address) {
        Ok(stream) => {
            stream.set_nonblocking(true).unwrap();
            alive_notifier.notify_status(AliveStatus::Connected);
            ConnectionState::Connected {
                controller_id,
                stream
            }
        }
        Err(_) => {
            alive_notifier.notify_status(AliveStatus::Disconnected);
            ConnectionState::Disconnected
        }
    }
}
