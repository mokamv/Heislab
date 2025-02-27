use crate::connection::client_pool::client_pool::ClientPool;
use crate::connection::connection_handle::ConnectionHandle;
use crate::connection::controller_state::ControllerState;
use crate::connection::controller_state::ControllerState::{BACKUP, MASTER};
use crate::connection::constants::ip_addresses::{CLIENT_UDP_LISTEN_ADDR, SERVER_TCP_ADDRESSES, SERVER_UDP_BIND_ADDR, SERVER_UDP_BROADCAST_ADDR};
use crate::connection::constants::{
    BIND_MAX_RETRY, BIND_RETRY_PERIOD, BROADCAST_PERIOD, UDP_BROADCAST_TIMEOUT,
};
use crate::connection::unix_socket::udp_socket_sharing_port;
use crate::log::log_client::ReliableLogSender;
use crate::log::LogLevel;
use crate::messages::{Message, DEFAULT_MESSAGE, MESSAGE_SIZE};
use std::cmp::Ordering;
use std::io::ErrorKind;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Instant;

pub fn init_udp_broadcasting(
    tcp_bound_to: SocketAddr,
    state: &Arc<Mutex<ControllerState>>,
    faulted: &Arc<Mutex<bool>>,
    id: u8,
    logger: ReliableLogSender,
) {
    let faulted = faulted.clone();
    let state = state.clone();

    let as_controller = Message::encode(Message::SocketAddress {
        id,
        state: MASTER,
        address: tcp_bound_to,
    });
    let as_backup = Message::encode(Message::SocketAddress {
        id,
        state: BACKUP,
        address: tcp_bound_to,
    });

    logger.send(
        "Starting broadcasting TCP Socket address for clients to use over UDP",
        LogLevel::INFO,
    );

    spawn(move || {
        let mut retry_counter: u32 = 0;

        'binding_loop: while retry_counter < BIND_MAX_RETRY {
            match udp_socket_sharing_port(SERVER_UDP_BIND_ADDR) {
                Ok(udp_socket) => {
                    if let Err(_broadcast_set_error) = udp_socket.set_broadcast(true) {
                        retry_counter += 1;
                    } else {
                        retry_counter = 0;

                        'broadcast_loop: loop {
                            if *faulted.lock().unwrap() {
                                break 'binding_loop;
                            }

                            match udp_socket.send_to(
                                match *state.lock().unwrap() {
                                    BACKUP => &as_backup,
                                    MASTER => &as_controller,
                                },
                                SERVER_UDP_BROADCAST_ADDR,
                            ) {
                                Ok(0) | Err(_) => {
                                    break 'broadcast_loop;
                                }
                                Ok(_) => {}
                            };

                            sleep(BROADCAST_PERIOD);
                        }
                    }
                }
                Err(_) => {
                    retry_counter += 1;
                }
            }
            if *faulted.lock().unwrap() {
                break 'binding_loop;
            }
            sleep(BIND_RETRY_PERIOD)
        }

        if !*faulted.lock().unwrap() {
            logger.send(
                "Failed to broadcast the TCP socket, this is a fatal error",
                LogLevel::ERROR,
            );
        }

        *faulted.lock().unwrap() = true;
    });
}

pub fn init_udp_reconciliation(
    state: &Arc<Mutex<ControllerState>>,
    faulted: &Arc<Mutex<bool>>,
    client_pool: &ClientPool,
    id: u8,
    logger: ReliableLogSender,
) {
    let faulted = faulted.clone();
    let state = state.clone();

    let mut client_pool = client_pool.clone();

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

                    let mut last_controller_message = Instant::now();

                    'while_bound: loop {
                        if *faulted.lock().unwrap() {
                            break 'unbound;
                        };
                        if !try_handle_packet(
                            &udp_socket,
                            &state,
                            id,
                            &logger,
                            &mut last_controller_message,
                        ) {
                            break 'while_bound;
                        };

                        if Instant::now().duration_since(last_controller_message)
                            > UDP_BROADCAST_TIMEOUT
                        {
                            logger.send(
                                "No UDP frames were received in some times. Becoming controller",
                                LogLevel::INFO,
                            );
                            *state.lock().unwrap() = MASTER;
                        }
                    }

                    client_pool.disconnect_all();
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

pub fn try_handle_packet(
    udp_socket: &UdpSocket,
    state: &Arc<Mutex<ControllerState>>,
    id: u8,
    logger: &ReliableLogSender,
    last_valid_controller_message: &mut Instant,
) -> bool {
    let mut received_message = DEFAULT_MESSAGE;

    match udp_socket.recv_from(&mut received_message) {
        // Check for udp frames coming from the server
        Ok((MESSAGE_SIZE, SERVER_UDP_BIND_ADDR)) => {
            if let Message::SocketAddress {
                id: other_id,
                state: other_state,
                ..
            } = Message::decode_message(&received_message)
            {
                match *state.lock().unwrap() {
                    BACKUP => {
                        match other_state {
                            // Do nothing when a backup receive another backup message.
                            BACKUP => {}

                            // Acknowledge controller
                            MASTER => *last_valid_controller_message = Instant::now()
                        }
                    }
                    MASTER => {
                        *last_valid_controller_message = Instant::now();
                        match other_state {
                            // Do nothing when a controller receive backup message
                            BACKUP => {}

                            // Reconciliation process
                            MASTER => {
                                match id.cmp(&other_id) {
                                    Ordering::Equal => {} // Ignores self message.
                                    Ordering::Less => {
                                        logger.send("Reconciliation: I'm staying master.", LogLevel::DEBUG);
                                    } // Should stay master (or ignore)
                                    Ordering::Greater => {
                                        //TODO LOOK INTO SYNCHRONISATION (SENDING DATA TO OTHER MASTER).
                                        *state.lock().unwrap() = BACKUP;
                                        logger.send("Reconciliation: I'm stepping down.", LogLevel::DEBUG);
                                    } // Become a backup, send current state to master
                                }
                            }
                        }
                    }
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

pub fn init_controller_tcp_listening(
    state: &Arc<Mutex<ControllerState>>,
    faulted: &Arc<Mutex<bool>>,
    client_pool: &ClientPool,
    logger: ReliableLogSender,
) -> SocketAddr {
    let listener = TcpListener::bind(&SERVER_TCP_ADDRESSES[..]).unwrap();
    let listener_bound_to = listener.local_addr().unwrap();

    let faulted = faulted.clone();
    let state = state.clone();
    let client_pool = client_pool.clone();

    spawn(move || {
        'client_accept: while !*faulted.lock().unwrap() {
            match listener.accept() {
                Ok((stream, address)) => {
                    match *state.lock().unwrap() {
                        BACKUP => {
                            logger.send(
                                "This node is in backup mode and shouldn't receive connection",
                                LogLevel::WARNING,
                            );
                            stream.shutdown(Shutdown::Both).unwrap()
                        }
                        MASTER => {
                            logger.send(
                                &format!("Received connection from {}", address),
                                LogLevel::DEBUG,
                            );
                            handle_connection(stream, &logger, &faulted, &client_pool);
                        }
                    };
                }
                Err(connection_error) => match connection_error.kind() {
                    ErrorKind::WouldBlock => {}
                    _ => {
                        break 'client_accept;
                    }
                },
            };
        }
        *faulted.lock().unwrap() = true;
    });

    listener_bound_to
}

fn handle_connection(
    stream: TcpStream,
    logger: &ReliableLogSender,
    faulted: &Arc<Mutex<bool>>,
    client_pool: &ClientPool,
) {
    let logger = logger.clone();
    let faulted = faulted.clone();
    let mut client_pool = client_pool.clone();

    spawn(move || {
        let temporary_handle =
            ConnectionHandle::new_temporary_connection_handler(stream, logger.clone(), &faulted);
        if let Err(error) = client_pool.handle_new_connection(temporary_handle) {
            logger.send(&format!("Disconnected because of {:?}", error.kind()), LogLevel::INFO);
        } else {
            logger.send("Client has been authenticated", LogLevel::DEBUG);
        };
    });
}
