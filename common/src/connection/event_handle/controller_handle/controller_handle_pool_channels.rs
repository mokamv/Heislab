use crate::config::CLIENT_COUNT;
use crate::connection::event_handle::controller_handle::controller_handle::{ClientMessage, ControllerHandleState, SentControllerMessage, Target};
use crate::connection::event_handle::handle_state::HandleState::{Connected, Disconnected};
use crate::connection::event_handle::handle_state::{ConnectionIdentifier, HandleState, HANDLE_ACK_UNINIT, HANDLE_HASH_UNINIT};
use crate::connection::udp_impl::shared_udp_socket::udp_socket_sharing_port;
use crate::connection::udp_impl::udp_ack_socket::UdpAckSocket;
use crate::constants::{BROADCAST_PERIOD, CONTROLLER_BC_ADDR, CONTROLLER_BC_BIND_ADDR, UDP_KEEP_ALIVE_PERIOD};
use crate::data_structures::controller_state::ControllerState;
use crate::data_structures::network::message::Message;
use crate::data_structures::network::message::Message::{Ack, KeepAlive};
use crate::data_structures::network::payload::{NetworkPayload, NetworkPayloadNode, TimedPayload};
use crossbeam_channel::{tick, Receiver, Sender};
use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::ops::{Deref, DerefMut};
use std::time::Instant;

pub(in super::super) struct ControllerHandlePoolChannels {
    controller_id: ConnectionIdentifier,
    udp_socket: RefCell<UdpSocket>,
    pub(in super::super) udp_broadcast_ticking: Receiver<Instant>,
    tcp_listening_address: Option<SocketAddr>,

    // Receive data from epoll to send it to handle via the pool
    pub(in super::super) from_epoll_to_pool: Receiver<TimedPayload>,

    // Receive handle state from the network.
    pub(in super::super) handle_state_receiver: Receiver<ControllerHandleState>,

    // Receive controller state from the controller.
    pub(in super::super) controller_state_receiver: Receiver<ControllerState>,

    // Send messages to both the other controller and the connected clients
    to_handle_from_client_pool: Sender<ClientMessage>,
    to_handle_from_sync: Sender<Message>,

    // Receive messages that need to be sent to one or all clients
    // Receive messages to be sent to the other controller (sync)
    pub(in super::super) from_handle_to_pool: Receiver<SentControllerMessage>,

    pub(in super::super) keep_alive_ticking: Receiver<Instant>,

    controller_state: RefCell<ControllerState>,
    sync_state: RefCell<HandleState>,
    clients_state: HashMap<ConnectionIdentifier, RefCell<HandleState>>
}

impl ControllerHandlePoolChannels {
    pub(in super) fn new(
        controller_id: ConnectionIdentifier,
        client_ids: [ConnectionIdentifier; CLIENT_COUNT],
        handle_state_receiver: Receiver<ControllerHandleState>,
        controller_state_receiver: Receiver<ControllerState>,
        from_epoll_to_pool: Receiver<TimedPayload>,
        to_handle_from_client_pool: Sender<ClientMessage>,
        to_handle_from_sync: Sender<Message>,
        from_handle_to_pool: Receiver<SentControllerMessage>,
    ) -> Self {
        let udp_socket = udp_socket_sharing_port(CONTROLLER_BC_BIND_ADDR)
            .unwrap();
        let _ = udp_socket.set_broadcast(true).unwrap();
        let _ = udp_socket.connect(CONTROLLER_BC_ADDR).unwrap();

        let now = Instant::now();
        let keep_alive_ticking = tick(UDP_KEEP_ALIVE_PERIOD);

        Self {
            keep_alive_ticking,
            controller_id,
            udp_socket: RefCell::new(udp_socket),
            udp_broadcast_ticking: tick(BROADCAST_PERIOD),
            tcp_listening_address: None,
            handle_state_receiver,
            controller_state_receiver,
            from_epoll_to_pool,
            to_handle_from_client_pool,
            to_handle_from_sync,
            from_handle_to_pool,
            controller_state: RefCell::new(
                ControllerState::Backup
            ),
            sync_state: RefCell::new(
                Disconnected {
                    since: now
                }
            ),
            clients_state: client_ids
                .into_iter()
                .map(|client_id| (
                    client_id,
                    RefCell::new(
                        Disconnected {
                            since: now,
                        }
                    )
                ))
                .collect()
        }
    }

    pub(in super::super) fn update_controller_state(&self, new_state: ControllerState) {
        *self.controller_state
            .borrow_mut() = new_state
    }

    pub(in super::super::super) fn broadcast_udp_frame(&self) {
        match self.udp_socket.borrow_mut().send(
            &Message::ControllerAddress {
                id: self.controller_id,
                state: *self.controller_state.borrow(),
                address: self.tcp_listening_address.unwrap(),
            }.encode()
        ) {
            Ok(0) => panic!("An error happened"),
            Err(error) => panic!("An error happened: {error}"),
            Ok(_) => {}
        }
    }

    pub(in super::super) fn send_keep_alive(
        &self,
        udp_socket: &mut UdpAckSocket
    ) {
        udp_socket.send_to_ignore_ack(
            NetworkPayload::new_uninit(
                KeepAlive,
                NetworkPayloadNode::Sync,
                NetworkPayloadNode::Sync
            )
        );

        for client_id in self.clients_state.keys() {
            let client_id = *client_id;

            udp_socket.send_to_ignore_ack(
                NetworkPayload::new_uninit(
                    KeepAlive,
                    NetworkPayloadNode::Controller { controller_id: self.controller_id },
                    NetworkPayloadNode::Client { client_id }
                )
            );
        }
    }

    pub(in super::super) fn update_listening_address(&mut self, listening_address: SocketAddr) {
        self.tcp_listening_address = Some(listening_address);
    }

    pub(in super::super) fn update_handle_state(
        &self,
        new_state: ControllerHandleState,
        udp_socket: &mut UdpAckSocket
    ) {
        let (
            sender,
            destination,
            current_state_ref,
            new_state
        ) = match new_state {
            ControllerHandleState::ClientState { client_id, state } =>
                (
                    NetworkPayloadNode::Controller { controller_id: self.controller_id },
                    NetworkPayloadNode::Client { client_id },
                    self.clients_state.get(&client_id).unwrap(),
                    state
                ),
            ControllerHandleState::SyncState { state } =>
                (
                    NetworkPayloadNode::Sync,
                    NetworkPayloadNode::Sync,
                    &self.sync_state,
                    state
                )
        };

        let borrowed_state = current_state_ref.borrow();
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
                self.send_to_handle(destination, Message::Connected)
            }

            (
                Connected { since, .. },
                Disconnected { since: disconnected_since }
            ) => {
                debug_assert!(since <= disconnected_since);
                udp_socket.clear_route(sender, destination);
                
                // Informs the handle of the new state
                self.send_to_handle(destination, Message::Disconnected);
            }
            (
                Disconnected { since },
                Connected {
                    since: connected_since,
                    address,
                    hash,
                    ..
                }
            ) => {
                debug_assert!(since <= connected_since);
                udp_socket.create_route(
                    sender,
                    destination,
                    *address,
                );

                if *hash != HANDLE_ACK_UNINIT {
                    self.send_to_handle(destination, Message::Connected);
                }
            }
            (Connected { .. }, Connected { .. }) => unreachable!("Disconnected should have been received first"),
            (Disconnected { .. }, Disconnected { .. }) => unreachable!("Connected should have been received first"),
        }
        drop(borrowed_state);
        current_state_ref.replace(new_state);
    }

    fn send_to_handle(&self, sender: NetworkPayloadNode, message: Message) {
       let _ = match sender {
            NetworkPayloadNode::Sync => self.to_handle_from_sync
                .send(message)
                .unwrap(),
            NetworkPayloadNode::Client { client_id } => self
                .to_handle_from_client_pool
                .send(
                    ClientMessage {
                        client_id,
                        message,
                    }
                )
                .unwrap(),
           _ => unreachable!()
        };
    }
    
    pub(in super::super) fn handle_message_from_handle(
        &self,
        sent_message: SentControllerMessage,
        udp_socket: &mut UdpAckSocket
    ) {
        let (
            borrowed_states,
            timed_message
        ) = match sent_message {
            SentControllerMessage::Sync { message } => {
                (
                    vec![(
                        &self.sync_state,
                        NetworkPayloadNode::Sync,
                        NetworkPayloadNode::Sync
                    )],
                    message
                )
            }
            SentControllerMessage::Client { target, message } => {
                match target {
                    Target::All => {
                        let vec = self.clients_state
                            .iter()
                            .map(|(client_id, handle_state)| { (
                                handle_state,
                                NetworkPayloadNode::Controller { controller_id: self.controller_id },
                                NetworkPayloadNode::Client { client_id: *client_id }
                            )
                            })
                            .collect();
                        (vec, message)
                    }
                    Target::Specific(client_id) => {
                        (
                            vec![(
                                self.clients_state.get(&client_id).unwrap(),
                                NetworkPayloadNode::Controller { controller_id: self.controller_id },
                                NetworkPayloadNode::Client { client_id }
                            )],
                            message
                        )
                    }
                }
            }
        };

        for (
            handle_state,
            sender ,
            destination
        ) in borrowed_states {
            let mut borrowed_state = handle_state.borrow_mut();

            if let Connected { since, .. } = borrowed_state.deref_mut() {
                if *since < timed_message.timestamp() {
                    let payload = NetworkPayload::new_uninit(
                        timed_message.message(),
                        sender,
                        destination
                    );
                    udp_socket.send_to(payload);
                }
            }
        }
    }

    pub(in super::super) fn handle_message_from_epoll(
        &self,
        message_from_socket: TimedPayload,
        udp_socket: &mut UdpAckSocket
    ) {
        let sender = message_from_socket.payload().sender();
        let destination = message_from_socket.payload().destination();
        let message = message_from_socket.payload().message();

        let handle_state = match destination {
            NetworkPayloadNode::Sync => &self.sync_state,
            NetworkPayloadNode::Controller { .. } => {
                let client_id = match sender {
                    NetworkPayloadNode::Client { client_id } => client_id,
                    _ => unreachable!()
                };
                self.clients_state.get(&client_id).unwrap()
            }
            _ => unreachable!()
        };

        if let Connected { since, hash, .. } = handle_state.borrow().deref() {
            if *since < message_from_socket.timestamp() {
                let payload = message_from_socket.payload();
                match message {
                    // Handle an ack message.
                    Ack => {
                        udp_socket.acknowledged_by(payload);
                    }
                    message => {
                        udp_socket.send_acknowledge_for(payload);
                        if *hash == payload.hash() &&
                            !udp_socket.has_payload_already_been_received(payload)
                        {
                            self.send_to_handle(sender, message);
                        }
                    }
                }
            }
        }
    }
}