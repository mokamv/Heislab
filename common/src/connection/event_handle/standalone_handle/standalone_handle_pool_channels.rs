use crate::connection::event_handle::handle_state::HandleState::{Connected, Disconnected};
use crate::connection::event_handle::handle_state::{ConnectionIdentifier, HandleState, HANDLE_ACK_UNINIT, HANDLE_HASH_UNINIT};
use crate::connection::udp_impl::udp_ack_socket::UdpAckSocket;
use crossbeam_channel::{tick, Receiver, Sender};
use std::cell::RefCell;
use std::ops::Deref;
use std::time::Instant;
use crate::constants::UDP_KEEP_ALIVE_PERIOD;
use crate::data_structures::network::message::{Message, TimedMessage};
use crate::data_structures::network::message::Message::KeepAlive;
use crate::data_structures::network::payload::{NetworkPayload, NetworkPayloadNode, TimedPayload};

pub(in super::super) struct StandaloneHandlePoolChannels {
    connection_id: ConnectionIdentifier,

    // Receive data from epoll to send it to handle via the pool
    pub(in super::super) from_epoll_to_pool: Receiver<TimedPayload>,
    pub(in super::super) handle_state_receiver: Receiver<HandleState>,

    // Send data from handle to socket (network)
    pub(in super::super) from_handle_to_pool: Receiver<TimedMessage>,
    // Send data to handle, from epoll via the pool
    to_handle_from_pool: Sender<Message>,

    // Support sending keep_alive messages
    pub(in super::super) keep_alive_ticking: Receiver<Instant>,

    state: RefCell<HandleState>
}


impl StandaloneHandlePoolChannels {
    pub(in super) fn new(
        connection_id: ConnectionIdentifier,
        from_epoll_to_pool: Receiver<TimedPayload>,
        handle_state_receiver: Receiver<HandleState>,
        from_handle_to_pool: Receiver<TimedMessage>,
        to_handle_from_pool: Sender<Message>,

    ) -> Self {
        let keep_alive_ticking = tick(UDP_KEEP_ALIVE_PERIOD);

        Self {
            keep_alive_ticking,
            connection_id,
            from_epoll_to_pool,
            handle_state_receiver,
            from_handle_to_pool,
            to_handle_from_pool,
            state: RefCell::new(Disconnected { since: Instant::now() }),
        }
    }

    pub(in super::super) fn send_keep_alive(
        &self,
        udp_socket: &mut UdpAckSocket
    ) {
        if let Connected {
            connected_to,
            ..
        } = &*self.state.borrow() {
            udp_socket.send_to_ignore_ack(
                NetworkPayload::new_uninit(
                    KeepAlive,
                    NetworkPayloadNode::Client { client_id: self.connection_id },
                    NetworkPayloadNode::Controller { controller_id: *connected_to }
                )
            )
        }
    }

    pub(in super::super) fn update_state(
        &self,
        new_state: HandleState,
        udp_socket: &mut UdpAckSocket
    ) {
        let borrowed_state = self.state.borrow();

        match (&*borrowed_state, &new_state) {
            (
                Connected {
                    hash,
                    since,
                    ..
                },
                Connected {
                    hash: definitive_hash,
                    since: definitive_since,
                    ..
                }
            ) if *hash == HANDLE_HASH_UNINIT && *definitive_hash != HANDLE_HASH_UNINIT => {
                debug_assert!(since <= definitive_since);
                let _ = self.to_handle_from_pool
                    .send(Message::Connected)
                    .unwrap();
            }
            (
                Connected {
                    since,
                    connected_to,
                    ..
                },
                Disconnected { since: disconnected_since }
            ) => {
                debug_assert!(since <= disconnected_since);
                udp_socket.clear_route(
                    NetworkPayloadNode::Client { client_id: self.connection_id },
                    NetworkPayloadNode::Controller { controller_id: *connected_to },
                );

                let _ = self.to_handle_from_pool
                    .send(Message::Disconnected)
                    .unwrap();
            }
            (
                Disconnected { since },
                Connected {
                    since: connected_since,
                    connected_to,
                    address,
                    hash,
                    .. }
            ) => {
                debug_assert!(since <= connected_since);
                udp_socket.create_route(
                    NetworkPayloadNode::Client { client_id: self.connection_id },
                    NetworkPayloadNode::Controller { controller_id: *connected_to },
                    *address
                );

                if *hash != HANDLE_ACK_UNINIT {
                    let _ = self.to_handle_from_pool
                        .send(Message::Connected)
                        .unwrap();
                }
            }

            (Connected { .. }, Connected { .. }) => unreachable!("Disconnected should have been received first"),
            (Disconnected { .. }, Disconnected { .. }) => unreachable!("Connected should have been received first"),
        }
        // Replaces the old state with the new state.
        drop(borrowed_state);
        self.state.replace(new_state);
    }

    pub(in super::super) fn handle_message_from_handle(
        &self,
        timed_message: TimedMessage,
        udp_socket: &mut UdpAckSocket
    ) {
        let borrowed_state = self.state.borrow();
        if let Connected {
            since,
            connected_to,
            ..
        } = borrowed_state.deref() {
            let timed_payload = TimedPayload::from(
                timed_message,
                NetworkPayloadNode::Client { client_id: self.connection_id },
                NetworkPayloadNode::Controller { controller_id: *connected_to }
            );

            if *since < timed_payload.timestamp() {
                udp_socket.send_to(timed_payload.payload())
            }
        }
    }

    pub(in super::super) fn handle_message_from_epoll(
        &self,
        message_from_socket: TimedPayload,
        udp_socket: &mut UdpAckSocket
    ) {
        if let Connected { since, hash, .. } = self.state.borrow().deref() {
            if *since < message_from_socket.timestamp() {
                let payload = message_from_socket.payload();
                match &payload.message() {
                    // Handle an ack message.
                    Message::Ack => {
                        udp_socket.acknowledged_by(payload);
                    }
                    message => {
                        udp_socket.send_acknowledge_for(payload);
                        if *hash == payload.hash()
                            && !udp_socket.has_payload_already_been_received(payload)
                        {
                            let _ = self.to_handle_from_pool.send(*message).unwrap();
                        }
                    }
                }
            }
        }
    }
}