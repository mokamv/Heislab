use crate::connection::client_pool::client_pool::ClientPool;
use crate::connection::connection_handle::handle::ConnectionHandle;
use crate::connection::connection_init::init_error::ErrorKind::{IdentificationTimedOut, UnexpectedMessageType, WrongController};
use crate::connection::connection_init::init_error::InitError;
use crate::connection::connection_init::init_server::ConnectionType::{Client, Controller};
use crate::connection::constants::ip_addresses::{SERVER_TCP_ADDRESSES, SERVER_UDP_BIND_ADDR, SERVER_UDP_BROADCAST_ADDR};
use crate::connection::constants::{BIND_MAX_RETRY, BIND_RETRY_PERIOD, BROADCAST_PERIOD, IDENTIFICATION_TIMEOUT};
use crate::connection::controller_state::ControllerState::{Backup, Master, MasterSteppingDown};
use crate::connection::controller_state::{ControllerState, ControllerStateNotifier};
use crate::connection::synchronisation::controller_link::ControllerLink;
use crate::connection::unix_socket::udp_socket_sharing_port;
use crate::messages::Message;
use crate::messages::Message::ControllerAddress;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Instant;
use faulted::{is_faulted, set_to_faulted};
use log::log_client::ReliableLogSender;
use log::LogLevel;

pub fn init_udp_broadcasting(
    tcp_bound_to: SocketAddr,
    state: ControllerStateNotifier,
    id: u8,
    logger: ReliableLogSender,
) {
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
                            if is_faulted() { break 'binding_loop; }

                            match udp_socket.send_to(
                                &ControllerAddress {
                                    id,
                                    state: state.current_state(),
                                    address: tcp_bound_to,
                                }.encode(), SERVER_UDP_BROADCAST_ADDR
                            ) {
                                Ok(0) | Err(_) => break 'broadcast_loop,
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
            if is_faulted() { break 'binding_loop; }
            sleep(BIND_RETRY_PERIOD)
        }

        if !is_faulted() {
            set_to_faulted("Failed to broadcast the TCP socket, this is a fatal error");
            logger.send(
                "Failed to broadcast the TCP socket, this is a fatal error",
                LogLevel::ERROR,
            );
        }
    });
}

pub fn init_controller_tcp_listening(
    controller_state: ControllerStateNotifier,
    controller_link: ControllerLink,
    client_pool: &ClientPool,
    logger: ReliableLogSender,
) -> SocketAddr {
    let listener = TcpListener::bind(&SERVER_TCP_ADDRESSES[..]).unwrap();
    let listener_bound_to = listener.local_addr().unwrap();

    let client_pool = client_pool.clone();

    spawn(move || {
        'client_accept: while !is_faulted() {
            match listener.accept() {
                Ok((stream, address)) => {
                    logger.send(
                        &format!("Received connection from {}", address),
                        LogLevel::DEBUG,
                    );
                    handle_new_connection(stream, &controller_state, &controller_link, &logger, &client_pool);
                }
                Err(connection_error) => match connection_error.kind() {
                    ErrorKind::WouldBlock => {}
                    _ => {
                        break 'client_accept;
                    }
                },
            };
        }
        set_to_faulted("TcpListener has been severed");
    });

    listener_bound_to
}

fn handle_new_connection(
    stream: TcpStream,
    controller_state_notifier: &ControllerStateNotifier,
    controller_link: &ControllerLink,
    logger: &ReliableLogSender,
    client_pool: &ClientPool
) {
    let logger = logger.clone();

    let controller_state_notifier = controller_state_notifier.clone();
    let controller_link = controller_link.clone();
    let client_pool = client_pool.clone();

    spawn(move || {
        // Generate a temporary connection
        let temporary_handle =
            ConnectionHandle::new_temporary_connection_handler(stream, logger.clone());

        // Handle the identification process
        let identification_result
            = handle_identification(temporary_handle, controller_state_notifier.current_state(), &logger);

        // Finalize connection and pass it to the appropriate definitive handler.
        handle_post_identification(identification_result, controller_link, client_pool, &logger);
    });
}


fn handle_identification(
    potential_handle: ConnectionHandle,
    controller_state: ControllerState,
    logger: &ReliableLogSender,
) -> Result<ConnectionType, InitError> {
    let message_receiver = potential_handle.borrow_receiver();
    let started_at = Instant::now();
    loop {
        match message_receiver.recv_timeout(IDENTIFICATION_TIMEOUT - started_at.elapsed() ) {
            Ok(Message::Connected) => {}

            // Handle the case of a client connection
            Ok(Message::ClientAuth { client_id }) => {
                return match controller_state {
                    Backup | MasterSteppingDown => {
                        logger.send(
                            "This node is in backup mode and shouldn't receive connection",
                            LogLevel::WARNING,
                        );
                        potential_handle.kill();
                        Err(InitError::new(WrongController))
                    }
                    Master => {
                        match potential_handle.extract_stream_and_kill() {
                            None => Err(InitError::new(IdentificationTimedOut)),
                            Some(client_stream) => Ok(Client { client_id, client_stream })
                        }
                    }
                }
            },

            // Handle the case of another controller connecting.
            Ok(Message::ControllerAuth { controller_id }) => {
                return match potential_handle.extract_stream_and_kill() {
                    None => Err(InitError::new(IdentificationTimedOut)),
                    Some(controller_stream) => Ok(Controller { controller_id, controller_stream })
                }
            },

            Ok(_) => {
                potential_handle.kill();
                return Err(InitError::new(UnexpectedMessageType))
            }
            Err(_too_late_to_identify) => {
                potential_handle.kill();
                return Err(InitError::new(IdentificationTimedOut))
            }
        }
    }
}

fn handle_post_identification(
    connection_type: Result<ConnectionType, InitError>,
    controller_link: ControllerLink,
    mut client_pool: ClientPool,
    logger: &ReliableLogSender
) {
    match connection_type {
        Ok(Controller { controller_id, controller_stream } ) => {
            if let Err(controller_link_error) = controller_link.connect_stream(controller_id, controller_stream) {
                logger.send(&format!("Cannot link the controller to a TCPStream because of: {controller_link_error:?}"), LogLevel::WARNING);
            }
        }

        Ok(Client { client_id, client_stream }) => {
            if let Err(client_pool_error) = client_pool.connect_stream(client_id, client_stream) {
                logger.send(&format!("Cannot connect to the client because of: {client_pool_error:?}"), LogLevel::WARNING);
            }
        }

        Err(init_error) => {
            logger.send(&format!("Cannot initialize the connection because of: {init_error:?}"), LogLevel::WARNING);
        }
    }
}

pub(in super::super) enum ConnectionType {
    Controller { controller_id: u8, controller_stream: TcpStream },
    Client { client_id: u8, client_stream: TcpStream }
}