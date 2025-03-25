use crate::connection::constants::ip_addresses::{COMMON_UDP_BIND_ADDR, HANDLE_UDP_LISTEN_ADDR};
use crate::connection::event_handle::controller_handle::controller_handle_epoll_channels::ControllerHandleEpollChannels;
use crate::connection::event_handle::handle_state::{ConnectionIdentifier, HandleState};
use crate::connection::event_handle::standalone_handle::standalone_handle_epoll_channels::StandaloneHandleEpollChannels;
use crate::connection::receiver::epoll::epoll::{add_ctl, epoll_create, epoll_wait, events_create, rearm_fd, BASE_GLOBAL_COUNTER};
use crate::connection::receiver::epoll::receiver_type::Receiver;
use crate::connection::receiver::wrapper::udp_broadcast_receiver::ErrorKind::RetryError;
use crate::connection::receiver::wrapper::udp_broadcast_receiver::UdpBroadcastReceiver;
use crate::connection::receiver::wrapper::udp_receiver::UdpDataReceiver;
use crate::messages::{PayloadNode, TimedPayload};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::os::fd::{AsRawFd, RawFd};
use std::thread::{Builder, JoinHandle};
use crate::data_struct::ControllerState;
use crate::data_struct::ControllerState::Master;

pub struct EpollReceiver {
    // Epoll directly related stuff (key, receivers and epoll_fd)
    epoll_fd: RawFd,
    key_global_counter: u64,
    epoll_contexts: HashMap<u64, Receiver>,

    // Manage standalone handle.
    standalone_handle: Option<StandaloneHandleEpollChannels>,

    // Manage controller handle
    controller_handle: Option<ControllerHandleEpollChannels>,
}

impl EpollReceiver {
    pub fn init() -> (Self, UdpSocket) {
        let epoll_fd = match epoll_create() {
            Ok(epoll_fd) => epoll_fd,
            Err(err) => panic!("Syscall epoll_wait failed with {err}")
        };

        let udp_data_socket = UdpDataReceiver::bind(COMMON_UDP_BIND_ADDR);
        let udp_sender = udp_data_socket.get_sender();

        let mut epoll_receiver = Self {
            epoll_fd,
            key_global_counter: BASE_GLOBAL_COUNTER,
            epoll_contexts: Default::default(),
            standalone_handle: None,
            controller_handle: None,
        };

        let udp_broadcast_receiver = Receiver::UDPBroadcastSocket(UdpBroadcastReceiver::bind(HANDLE_UDP_LISTEN_ADDR));
        epoll_receiver.add_receiver(udp_broadcast_receiver);

        let udp_data_receiver = Receiver::UDPDataSocket(udp_data_socket);
        epoll_receiver.add_receiver(udp_data_receiver);

        (epoll_receiver, udp_sender)
    }

    pub(in super::super::super) fn with_standalone_handle(&mut self, handle_receiver: StandaloneHandleEpollChannels) {
        assert!(self.standalone_handle.is_none());
        self.standalone_handle = Some(handle_receiver);
    }

    pub(in super::super::super) fn with_controller_handle(
        &mut self,
        controller_handle_channels: ControllerHandleEpollChannels
    ) {
        assert!(self.controller_handle.is_none());
        self.controller_handle = Some(controller_handle_channels);
    }

    fn standalone_handle_address_broadcast(
        &mut self,
        recv_controller_id: ConnectionIdentifier,
        recv_controller_state: ControllerState,
        recv_address: SocketAddr
    ) {
        // Ignore when standalone is not enabled
        if self.standalone_handle.is_none() {
            return;
        }

        // Ignore non master controller
        if recv_controller_state != Master {
            return;
        }

        // Get standalone handle instance
        let standalone_handle = self
            .standalone_handle
            .as_ref()
            .unwrap();

        // Match the current handle state
        match standalone_handle.get_handle_state() {
            HandleState::Connected { connected_to, .. }=> {
                // Connect to the lowest id controller.
                if connected_to > recv_controller_id {
                    standalone_handle.send_disconnect();
                    standalone_handle.send_connect(
                        recv_controller_id,
                        recv_address
                    )
                }
            }
            HandleState::Disconnected { .. } => {
                // Connect a standalone (elevator client) to the master.
                standalone_handle.send_connect(
                    recv_controller_id,
                    recv_address
                )
            }
        }
    }

    fn sync_handle_address_broadcast(
        &mut self,
        recv_controller_id: ConnectionIdentifier,
        recv_address: SocketAddr
    ) {
        // Ignore when controller is not enabled
        if self.controller_handle.is_none() {
            return;
        }

        // Get controller handle instance
        let controller_handle = self
            .controller_handle
            .as_ref()
            .unwrap();

        // Ignore self broadcasting
        if recv_controller_id == controller_handle.get_controller_id() {
            return;
        }

        // Match the current handle state
        match controller_handle.get_sync_handle_state() {
            // Ignore already connected
            HandleState::Connected { .. } => {}
            HandleState::Disconnected { .. } => {
                // Connect two controllers together.
                controller_handle.send_sync_connect(
                    recv_controller_id,
                    recv_address
                )
            }
        }
    }

    fn check_timeout(&mut self) {
        if let Some(standalone_handle) = self
            .standalone_handle
            .as_ref()
        {
            standalone_handle.check_timeout();
        }

        if let Some(controller_handle) = self
            .controller_handle
            .as_ref()
        {
            controller_handle.check_timeout();
        }
    }

    fn add_receiver(&mut self, receiver: Receiver) -> u64 {
        self.key_global_counter += 1;
        add_ctl(self.epoll_fd, receiver.as_raw_fd(), self.key_global_counter)
            .expect("Failed to add this receiver stream to epoll");
        let old = self.epoll_contexts.insert(
            self.key_global_counter,
            receiver
        );
        debug_assert!(old.is_none());
        self.key_global_counter
    }

    pub fn start_receiver_thread_from_builder(mut self) -> JoinHandle<()> {
        let builder = Builder::new().name("Epoll".to_string());
        builder.spawn(move || {
            let mut events = events_create();

            loop {
                epoll_wait(self.epoll_fd, &mut events);

                for ev in &events {
                    let receiver_key = ev.u64;

                    if let Some(receiver) = self.epoll_contexts.get_mut(&receiver_key) {
                        match receiver {
                            // Handle UDP Broadcasting.
                            Receiver::UDPBroadcastSocket(receiver) => {
                                rearm_fd(self.epoll_fd, receiver.as_raw_fd(), receiver_key);
                                match receiver.handle_broadcast() {
                                    Ok((controller_id, state, address)) => {
                                        self.standalone_handle_address_broadcast(controller_id, state, address);
                                        self.sync_handle_address_broadcast(controller_id, address);
                                    }
                                    Err(error) if error.kind() == RetryError => {},
                                    Err(_faulting_error) => {} //TODO PANIC / FAULT
                                };
                            }

                            Receiver::UDPDataSocket(receiver) => {
                                rearm_fd(self.epoll_fd, receiver.as_raw_fd(), receiver_key);

                                // Receive the payload for the message.
                                let message = receiver.recv_message();

                                if let Ok((payload, sender_address)) = message {
                                    self.handle_keep_alive(payload, sender_address);
                                    self.handle_data_payload(payload);
                                }
                            }
                        }
                    }
                }
                self.check_timeout();
            }
        }).unwrap()
    }

    fn handle_keep_alive(&self, timed_payload: TimedPayload, recv_address: SocketAddr) {
        let payload = timed_payload.payload();
        let recv_ack = payload.ack();
        let recv_hash = payload.hash();
        let message = payload.message();

        //TODO OPTI
        if message.is_keep_alive() {
            match (payload.sender(), payload.destination()) {
                (
                    PayloadNode::Client { client_id },
                    PayloadNode::Controller { .. }
                ) => {
                    let controller_handle = self.controller_handle
                        .as_ref()
                        .unwrap();

                    match controller_handle.get_client_handle_state(client_id) {
                        HandleState::Connected { .. } => {
                            controller_handle
                                .update_client_connected_state(
                                    client_id,
                                    recv_ack,
                                    recv_hash
                                );
                        }
                        HandleState::Disconnected { .. } => {
                            controller_handle.send_client_connect(
                                client_id,
                                recv_address,
                                recv_ack,
                                recv_hash
                            )
                        }
                    }
                }

                (
                    PayloadNode::Controller { controller_id },
                    PayloadNode::Client { client_id }
                ) => {
                    let standalone_handle = self
                        .standalone_handle
                        .as_ref()
                        .unwrap();
                    debug_assert_eq!(client_id, standalone_handle.get_connection_id());

                    match standalone_handle.get_handle_state() {
                        HandleState::Connected { .. } => {
                            standalone_handle
                                .update_connected_state(
                                    controller_id,
                                    recv_ack,
                                    recv_hash
                                );
                        }
                        // Do nothing, UDP BC is responsible for creating initializing this side.
                        HandleState::Disconnected { .. } => {}
                    }
                }

                (
                    PayloadNode::Sync,
                    PayloadNode::Sync
                ) => {
                    // Get standalone handle instance
                    let controller_handle = self
                        .controller_handle
                        .as_ref()
                        .unwrap();

                    match controller_handle.get_sync_handle_state() {
                        HandleState::Connected { .. } => {
                            controller_handle
                                .update_sync_connected_state(
                                    recv_ack,
                                    recv_hash
                                )
                        }
                        // Do nothing, broadcast is responsible for creating this side
                        HandleState::Disconnected { .. } => {}
                    }
                }

                // TODO OTHERS
                _ => {
                    println!("RECEIVED NON DESCRIPTIVE DATA")
                }
            }
        }
    }

    fn handle_data_payload(&self, timed_payload: TimedPayload) {
        let payload = timed_payload.payload();
        let message = payload.message();

        // Ignore keep alive TODO OPTI
        if message.is_keep_alive() {
            return;
        }

        match (payload.sender(), payload.destination()) {
            (
                PayloadNode::Client { client_id },
                PayloadNode::Controller { .. }
            ) => {
                let controller_handle = self.controller_handle
                    .as_ref()
                    .unwrap();

                controller_handle
                    .send_client_message_to_pool(
                        client_id,
                        timed_payload
                    );
            }

            (
                PayloadNode::Controller { controller_id },
                PayloadNode::Client { client_id }
            ) => {
                let standalone_handle = self.standalone_handle
                    .as_ref()
                    .unwrap();
                debug_assert_eq!(client_id, standalone_handle.get_connection_id());
                standalone_handle
                    .send_message(
                        timed_payload,
                        controller_id
                    );
            }

            (
                PayloadNode::Sync,
                PayloadNode::Sync
            ) => {
                let controller_handle = self.controller_handle
                    .as_ref()
                    .unwrap();

                controller_handle.send_sync_message_to_pool(timed_payload);
            }


            //TODO OTHERS
            _ => unreachable!()
        }
    }
}