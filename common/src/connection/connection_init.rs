pub mod init_server {
    use crate::connection::connection_handle::ConnectionHandle;
    use crate::connection::constants::ip_addresses::{SERVER_TCP_ADDRESS, SERVER_UDP_BIND_ADDR, SERVER_UDP_BROADCAST_ADDR};
    use crate::connection::constants::{BIND_MAX_RETRY, BIND_RETRY_PERIOD, BROADCAST_PERIOD};
    use crate::messages::Message;
    use std::io::ErrorKind;
    use std::net::{TcpListener, TcpStream, UdpSocket};
    use std::sync::{Arc, Mutex};
    use std::thread::{sleep, spawn};
    use crate::connection::client_pool::client_pool::ClientPool;
    use crate::log::log_client::ReliableLogSender;
    use crate::log::LogLevel;

    pub fn init_server_udp_broadcasting(faulted: &Arc<Mutex<bool>>, logger: ReliableLogSender) {
        let faulted = faulted.clone();

        let keep_alive = Message::encode(Message::SocketAddress { address: SERVER_TCP_ADDRESS });

        logger.send("Starting broadcasting TCP Socket address for clients to use", LogLevel::INFO);

        spawn(move || {
            let mut retry_counter: u32 = 0;

            'binding_loop: while retry_counter < BIND_MAX_RETRY {
                match UdpSocket::bind(SERVER_UDP_BIND_ADDR) {
                    Ok(udp_socket) => {
                        if let Err(_broadcast_set_error) = udp_socket.set_broadcast(true) {
                            retry_counter += 1;
                        } else {
                            retry_counter = 0;

                            'broadcast_loop: loop {
                                if *faulted.lock().unwrap() { break 'binding_loop }
                                
                                match udp_socket.send_to(&keep_alive, SERVER_UDP_BROADCAST_ADDR) {
                                    Ok(0) | Err(_) => {
                                        break 'broadcast_loop;
                                    },
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
                if *faulted.lock().unwrap() { break 'binding_loop }
                sleep(BIND_RETRY_PERIOD)
            }

            if !*faulted.lock().unwrap() {
                logger.send("Failed to broadcast the TCP socket, this is a fatal error", LogLevel::ERROR);
            }

            *faulted.lock().unwrap() = true;
        });
    }

    pub fn init_server_tcp_listening(faulted: &Arc<Mutex<bool>>, client_pool: &ClientPool, logger: ReliableLogSender) {
        let listener = TcpListener::bind(SERVER_TCP_ADDRESS).unwrap();
        let faulted = faulted.clone();
        let client_pool = client_pool.clone();

        spawn(move || {
            'client_accept: while !*faulted.lock().unwrap() {
                match listener.accept() {
                    Ok((stream, address)) => {
                        logger.send(&format!("Received connection from {}", address), LogLevel::DEBUG);
                        handle_connection(stream, &logger, &faulted, &client_pool);
                    }
                    Err(connection_error) => {
                        match connection_error.kind() {
                            ErrorKind::WouldBlock => {},
                            _ => {
                                break 'client_accept;
                            }
                        }
                    }
                };
            }
            *faulted.lock().unwrap() = true;
        });
    }

    fn handle_connection(stream: TcpStream, logger: &ReliableLogSender, faulted: &Arc<Mutex<bool>>, client_pool: &ClientPool) {
        let logger = logger.clone();
        let faulted = faulted.clone();
        let mut client_pool = client_pool.clone();

        spawn(move || {
            let connection =
                ConnectionHandle::new_server_connection_handler(stream, logger.clone(), &faulted);
            if let Err(error) = client_pool.handle_connection(connection) {
                logger.send(&format!("Disconnected because of {:?}", error.kind()), LogLevel::INFO); //TODO MANAGE ERROR
            } else {
                logger.send("Client has been authenticated", LogLevel::DEBUG);
            };
        });
    }
}
pub(super) mod init_client {
    use crate::connection::connection_channel::AliveStatusNotifier;
    use crate::connection::connection_handle::{ConnectionHandle, ConnectionState};
    use crate::connection::constants::ip_addresses::{CLIENT_UDP_LISTEN_ADDR, SERVER_UDP_BIND_ADDR};
    use crate::connection::constants::{BIND_MAX_RETRY, BIND_RETRY_PERIOD, BROADCAST_PERIOD, UDP_BROADCAST_TIMEOUT};
    use crate::connection::unix_socket::udp_socket_sharing_port;
    use crate::messages::{Message, DEFAULT_MESSAGE, MESSAGE_SIZE};
    use std::io::ErrorKind;
    use std::net::{SocketAddr, TcpStream, UdpSocket};
    use std::ops::Deref;
    use std::sync::{Arc, Mutex, RwLock};
    use std::thread::{sleep, spawn};
    use std::time::Instant;
    use crate::log::log_client::ReliableLogSender;
    use crate::log::LogLevel;

    pub fn update_main_server_loop(connection_handler: &mut ConnectionHandle, logger: &ReliableLogSender, faulted: &Arc<Mutex<bool>>) {
        let logger = logger.clone();
        let faulted = faulted.clone();
        let connection_state = connection_handler.connection_state.clone();
        let alive_notifier = connection_handler.alive_status_notifier();

        let mut bind_retry_count = 0u32;
        spawn(move || {
            'unbound: while bind_retry_count < BIND_MAX_RETRY {
                if *faulted.lock().unwrap() { break 'unbound }

                match udp_socket_sharing_port(CLIENT_UDP_LISTEN_ADDR) {
                    Ok(udp_socket) => {
                        if let Err(_cannot_set_timeout) = udp_socket.set_read_timeout(Some(BROADCAST_PERIOD)) {
                            println!("Timeout duration cannot be 0");
                            break 'unbound
                        }

                        bind_retry_count = 0;

                        println!("Successfully bounded to UDP Socket. Starting to receive frames.");

                        let mut last_valid_message = Instant::now();

                        'while_bound: loop {
                            if *faulted.lock().unwrap() { break 'unbound };
                            if !try_handle_packet(&udp_socket, &connection_state, &alive_notifier, &logger, &mut last_valid_message) {
                                break 'while_bound
                            };

                            if let ConnectionState::Connected(_) = connection_state.read().unwrap().deref() {
                                if Instant::now().duration_since(last_valid_message) > UDP_BROADCAST_TIMEOUT {
                                    logger.send("No UDP frames were received in some times. Considering the connection as timed out", LogLevel::WARNING);
                                    ConnectionHandle::disconnect(&alive_notifier, &mut connection_state.write().unwrap());
                                }
                            }
                        }

                        // Always disconnect if unbound from udp.
                        ConnectionHandle::disconnect(&alive_notifier, &mut connection_state.write().unwrap());
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
                if let Message::SocketAddress { address } = Message::decode_message(&received_message) {
                    *last_valid_message = Instant::now();
                    try_to_connect(connection_state, alive_notifier, address, logger);
                    true
                } else {
                    logger.send("The server sent an non address packet over the udp broadcast address", LogLevel::ERROR);
                    false
                }
            }

            Ok((_, SERVER_UDP_BIND_ADDR)) => {
                logger.send("Message of incorrect size received", LogLevel::ERROR);
                false
            }

            // Discarding every non-server packets
            Ok((_, _)) => { true },

            // Explicit error
            Err(error) => {
                match error.kind() {
                    // Here, blocking is equivalent to timing out.
                    ErrorKind::WouldBlock |
                    ErrorKind::TimedOut => true,

                    // All other kinds of errors.
                    _ => false
                }
            }
        }
    }

    fn try_to_connect(
        connection_state: &Arc<RwLock<ConnectionState>>,
        alive_notifier: &AliveStatusNotifier,
        address: SocketAddr,
        logger: &ReliableLogSender
    ) {
        // Check if the connection is already established
        if let ConnectionState::Connected(current_conn) = connection_state.read().unwrap().deref() {
            if let Ok(current_socket_addr) = current_conn.peer_addr() {
                if current_socket_addr == address { return; }
            }
        }

        // If current connection is in fact dead, try to establish new connection
        ConnectionHandle::disconnect(alive_notifier, &mut connection_state.write().unwrap());
        *connection_state.write().unwrap() = try_to_get_socket(address, alive_notifier);

        match connection_state.read().unwrap().deref() {
            ConnectionState::Connected(socket) => {
                logger.send(&format!("Connected to {:?}", socket.peer_addr()), LogLevel::INFO)
            },
            _ => {
                logger.send("Disconnected", LogLevel::INFO)
            }
        }
    }

    fn try_to_get_socket(address: SocketAddr, alive_notifier: &AliveStatusNotifier) -> ConnectionState {
        match TcpStream::connect(address) {
            Ok(tcp_stream) => {
                tcp_stream.set_nonblocking(true).unwrap();
                alive_notifier.is_connected(true);
                ConnectionState::Connected(tcp_stream)
            },
            Err(_) => {
                alive_notifier.is_connected(false);
                ConnectionState::Disconnected
            }
        }
    }
}