use crate::connection::client_pool::client_pool::ClientPool;
use crate::connection::connection_handle::handle::ConnectionHandleMutator;
use crate::connection::constants::ip_addresses::{SERVER_UDP_BIND_ADDR, UDP_LISTEN_ADDR};
use crate::connection::constants::{BIND_MAX_RETRY, BIND_RETRY_PERIOD, BROADCAST_PERIOD, UDP_BROADCAST_TIMEOUT};
use crate::connection::controller_state::ControllerState::Master;
use crate::connection::controller_state::ControllerStateNotifier;
use crate::connection::unix_socket::udp_socket_sharing_port;
use crate::messages::Message::ControllerAddress;
use crate::messages::{Message, DEFAULT_MESSAGE, MESSAGE_SIZE};
use std::io::ErrorKind;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Instant;
use log::LogLevel;

pub fn listen_and_synchronize(
    connection_handle_mutator: ConnectionHandleMutator,
    controller_state: ControllerStateNotifier,
    mut client_pool: ClientPool,
    faulted: &Arc<Mutex<bool>>,
) {
    let temp_logger = connection_handle_mutator.logger().clone();
    let faulted = faulted.clone();

    let mut bind_retry_count = 0u32;
    spawn(move || {
        'unbound: while bind_retry_count < BIND_MAX_RETRY {

            if *faulted.lock().unwrap() { break 'unbound; }

            match udp_socket_sharing_port(UDP_LISTEN_ADDR) {
                Ok(udp_socket) => {
                    if let Err(_cannot_set_timeout) = udp_socket.set_read_timeout(Some(BROADCAST_PERIOD))
                    {
                        println!("Timeout duration cannot be 0");
                        break 'unbound;
                    }

                    bind_retry_count = 0;

                    println!("Successfully bounded to UDP Socket. Starting to receive frames.");

                    let mut last_controller_message_instant = Instant::now();

                    'while_bound: loop {
                        if *faulted.lock().unwrap() { break 'unbound; };

                        if !try_handling_udp_packet(
                            &controller_state,
                            &udp_socket,
                            &connection_handle_mutator,
                            &mut last_controller_message_instant
                        ) {
                            break 'while_bound;
                        };


                        if last_controller_message_instant.elapsed() > UDP_BROADCAST_TIMEOUT
                        {
                            // In case of timeout, disconnect the tcp connection as well.
                            if connection_handle_mutator.is_connected() { connection_handle_mutator.disconnect(); }

                            controller_state.change_state(Master);
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

        temp_logger.send("Cannot bind to UDP", LogLevel::ERROR);
        *faulted.lock().unwrap() = true;
    });
}

pub fn try_handling_udp_packet(
    controller_state: &ControllerStateNotifier,
    udp_socket: &UdpSocket,
    connection_handle_mutator: &ConnectionHandleMutator,
    last_valid_controller_message: &mut Instant,
) -> bool {
    let mut received_message = DEFAULT_MESSAGE;

    match udp_socket.recv_from(&mut received_message) {
        // Check for udp frames coming from the server
        Ok((MESSAGE_SIZE, SERVER_UDP_BIND_ADDR)) => {
            if let ControllerAddress {
                state: other_state,
                address: other_address,
                id: other_id
            } = Message::decode_message(&received_message)
            {
                if controller_state.controller_id() != other_id {
                    if other_state == Master {
                        *last_valid_controller_message = Instant::now();
                    }

                    // Highest id initiate the connection.
                    if controller_state.controller_id() > other_id {
                        connection_handle_mutator.try_to_connect_to(other_address, other_id);
                    }
                } else {
                    if controller_state.current_state() == Master {
                        *last_valid_controller_message = Instant::now();
                    }
                }
                true
            } else {
                connection_handle_mutator.logger().send(
                    "The server sent an non address packet over the udp broadcast address",
                    LogLevel::ERROR,
                );
                false
            }
        }

        Ok((_, SERVER_UDP_BIND_ADDR)) => {
            connection_handle_mutator.logger().send("Message of incorrect size received", LogLevel::ERROR);
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