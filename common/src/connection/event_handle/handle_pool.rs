use crate::connection::event_handle::controller_handle::controller_handle::{ControllerHandle, ControllerHandleBuilder, ControllerHandleConfiguration};
use crate::connection::event_handle::controller_handle::controller_handle_pool_channels::ControllerHandlePoolChannels;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::connection::event_handle::standalone_handle::standalone_handle::{StandaloneHandle, StandaloneHandleBuilder};
use crate::connection::event_handle::standalone_handle::standalone_handle_pool_channels::StandaloneHandlePoolChannels;
use crate::connection::receiver::epoll::epoll_receiver::EpollReceiver;
use crate::connection::udp_impl::udp_ack_socket::UdpAckSocket;
use crate::messages::PayloadNode;
use crossbeam_channel::{Receiver, Select};
use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::thread::{Builder, JoinHandle};
use std::time::Instant;

pub struct HandlePool {
    udp_socket: RefCell<UdpAckSocket>,
    standalone_handle: Option<StandaloneHandlePoolChannels>,
    controller_handle: Option<ControllerHandlePoolChannels>,
}

impl HandlePool {
    pub fn init(udp_socket: UdpSocket) -> Self {
        Self {
            udp_socket: RefCell::new(UdpAckSocket::from(udp_socket)),
            standalone_handle: None,
            controller_handle: None }
    }

    pub fn with_standalone_handle(
        &mut self,
        epoll_receiver: &mut EpollReceiver,
        connection_id: ConnectionIdentifier,
    ) -> StandaloneHandle {
        assert!(self.standalone_handle.is_none());
        // Generates handle
        let mut handle = StandaloneHandleBuilder::new(connection_id);
        // Adds to pool
        self.standalone_handle = Some(handle.take_handle_pool_channels());
        // Adds to epoll_receiver
        epoll_receiver.with_standalone_handle(handle.take_handle_epoll_channels());
        // Converts into usable handle and returns.
        handle.into_handle()
    }

    pub fn with_controller_handle(
        &mut self,
        epoll_receiver: &mut EpollReceiver,
        bind_address: SocketAddr,
        config: ControllerHandleConfiguration,
    ) -> ControllerHandle {
        assert!(self.controller_handle.is_none());
        // Generates handle builder
        let mut handle = ControllerHandleBuilder::new(config);
        // Adds to pool
        self.controller_handle = Some(handle.take_controller_handle_pool_channels());
        // Adds to epoll_receiver
        epoll_receiver
            .with_controller_handle(handle.take_controller_handle_epoll_channels());
        // Update the TCP Listener address
        self.controller_handle
            .as_mut()
            .unwrap()
            .update_listening_address(bind_address);
        // Converts into usable handle and returns.
        handle.into_controller_handle()
    }

    pub fn start_handle_pool_thread(self) -> JoinHandle<()> {
        assert!(self.controller_handle.is_some() || self.standalone_handle.is_some(),
                "You need to setup at least one handle (client or controller)");

        let builder = Builder::new().name("HandlePool".to_string());
        builder.spawn(move || {
            // Starts select loop
            loop {
                let mut pool = InternalHandlePool::new();
                // Initialize the selector
                let mut selector: Select = Select::new();
                // Initializes controller handle (if enabled).
                if let Some(controller_handle) = &self.controller_handle {
                    pool.initialize_controller_handle_selector(controller_handle, &mut selector);
                }
                // Initializes standalone handle (if enabled)
                if let Some(standalone_handle) = &self.standalone_handle {
                    pool.initialize_standalone_handle_selector(standalone_handle, &mut selector);
                }

                let current_resends = self.udp_socket.borrow().get_retry_receivers();
                pool.initialize_resend_selector(&current_resends, &mut selector);

                let selected = selector.select();
                let selected_id = selected.index();

                let operation_type =
                    pool.operation_mapping
                        .get(&selected_id)
                        .unwrap();

                match *operation_type {
                    // Handle message the client want to send.
                    OperationType::StandaloneMessageFromHandleToPool => {
                        let standalone_handle = self
                            .standalone_handle
                            .as_ref()
                            .unwrap();
                        let message = selected
                            .recv(&standalone_handle.from_handle_to_pool)
                            .unwrap();
                        standalone_handle
                            .handle_message_from_handle(
                                message,
                                &mut *self.udp_socket.borrow_mut()
                            );
                    },
                    OperationType::ControllerMessageFromHandleToPool => {
                        let controller_handle = self
                            .controller_handle
                            .as_ref()
                            .unwrap();
                        let sent_message = selected
                            .recv(&controller_handle.from_handle_to_pool)
                            .unwrap();
                        controller_handle
                            .handle_message_from_handle(
                                sent_message,
                                &mut *self.udp_socket.borrow_mut()
                            );
                    }

                    // Handle received message from network.
                    OperationType::StandaloneMessageFromEpollToPool => {
                        let standalone_handle = self
                            .standalone_handle
                            .as_ref()
                            .unwrap();
                        let timed_message = selected
                            .recv(&standalone_handle.from_epoll_to_pool)
                            .unwrap();
                        standalone_handle
                            .handle_message_from_epoll(
                                timed_message,
                                &mut *self.udp_socket.borrow_mut()
                            );
                    }
                    OperationType::ControllerMessageFromEpollToPool => {
                        let controller_handle = self
                            .controller_handle
                            .as_ref()
                            .unwrap();
                        let received_message = selected
                            .recv(&controller_handle.from_epoll_to_pool)
                            .unwrap();
                        controller_handle
                            .handle_message_from_epoll(
                                received_message,
                                &mut *self.udp_socket.borrow_mut()
                            );
                    }

                    // Keep track of connection status.
                    OperationType::StandaloneHandleStateReceive => {
                        let standalone_handle = self.standalone_handle.as_ref().unwrap();
                        let new_state = selected
                            .recv(&standalone_handle.handle_state_receiver)
                            .unwrap();

                        standalone_handle.update_state(
                            new_state,
                            &mut *self.udp_socket.borrow_mut()
                        );
                    }
                    OperationType::ControllerHandleStateReceive => {
                        let controller_handle = self.controller_handle.as_ref().unwrap();
                        let new_state = selected
                            .recv(&controller_handle.handle_state_receiver)
                            .unwrap();

                        controller_handle.update_handle_state(
                            new_state,
                            &mut *self.udp_socket.borrow_mut()
                        );
                    }

                    OperationType::ControllerStateReceive => {
                        let controller_handle = self.controller_handle.as_ref().unwrap();
                        let new_state = selected
                            .recv(&controller_handle.controller_state_receiver)
                            .unwrap();

                        controller_handle.update_controller_state(
                            new_state
                        )
                    }

                    // Udp Broadcast (if enabled)
                    OperationType::ControllerBroadcastTicking => {
                        let controller_handle = self.controller_handle.as_ref().unwrap();
                        let _ = selected.recv(&controller_handle.udp_broadcast_ticking).unwrap();
                        controller_handle.broadcast_udp_frame();
                    }

                    // Send keep alive periodically from the standalone connection.
                    OperationType::StandaloneKeepAliveTicking => {
                        let standalone_handle = self.standalone_handle.as_ref().unwrap();
                        let _ = selected
                            .recv(&standalone_handle.keep_alive_ticking)
                            .unwrap();
                        standalone_handle.send_keep_alive(
                            &mut *self.udp_socket.borrow_mut()
                        )
                    }

                    // Send keep alive periodically from the controller connections.
                    OperationType::ControllerKeepAliveTicking => {
                        let controller_handle = self.controller_handle.as_ref().unwrap();
                        let _ = selected
                            .recv(&controller_handle.keep_alive_ticking)
                            .unwrap();
                        controller_handle.send_keep_alive(
                            &mut *self.udp_socket.borrow_mut()
                        )
                    }

                    // Operation resends (when packet has been dropped)
                    OperationType::UdpResend { receiver_index } => {
                        let (
                            sender,
                            destination,
                            recv
                        ) = current_resends.get(receiver_index).unwrap();

                        let _ = selected
                            .recv(recv)
                            .unwrap();

                        self.udp_socket
                            .borrow_mut()
                            .resend(*sender, *destination);
                    }
                }
            }
        }).unwrap()
    }
}

struct InternalHandlePool {
    operation_mapping: HashMap<usize, OperationType>,
}

impl InternalHandlePool {
    fn new() -> Self {
        Self {
            operation_mapping: Default::default(),
        }
    }

    fn initialize_standalone_handle_selector<'a>(&mut self, handle: &'a StandaloneHandlePoolChannels, selector: &mut Select<'a>) {
        let selected_fh_tp = selector.recv(&handle.from_handle_to_pool);
        let selected_fe_tp = selector.recv(&handle.from_epoll_to_pool);
        let selected_sr = selector.recv(&handle.handle_state_receiver);
        let selected_kat = selector.recv(&handle.keep_alive_ticking);

        let _fh_ts = self.operation_mapping
            .insert(selected_fh_tp, OperationType::StandaloneMessageFromHandleToPool)
            .is_none();
        debug_assert!(_fh_ts);
        let _fe_tp = self.operation_mapping
            .insert(selected_fe_tp, OperationType::StandaloneMessageFromEpollToPool)
            .is_none();
        debug_assert!(_fe_tp);
        let _sr = self.operation_mapping
            .insert(selected_sr, OperationType::StandaloneHandleStateReceive)
            .is_none();
        debug_assert!(_sr);
        let _kat = self.operation_mapping
            .insert(selected_kat, OperationType::StandaloneKeepAliveTicking)
            .is_none();
        debug_assert!(_kat);
    }

    fn initialize_controller_handle_selector<'a>(&mut self, handle: &'a ControllerHandlePoolChannels, selector: &mut Select<'a>) {
        let selected_udp_bc = selector.recv(&handle.udp_broadcast_ticking);
        let selected_c_fh_tp = selector.recv(&handle.from_handle_to_pool);
        let selected_c_fe_tp = selector.recv(&handle.from_epoll_to_pool);
        let selected_c_hsr = selector.recv(&handle.handle_state_receiver);
        let selected_c_sr = selector.recv(&handle.controller_state_receiver);
        let selected_kat = selector.recv(&handle.keep_alive_ticking);

        let _udp_bc = self.operation_mapping
            .insert(selected_udp_bc, OperationType::ControllerBroadcastTicking)
            .is_none();
        debug_assert!(_udp_bc);
        let _c_fh_tp = self.operation_mapping
            .insert(selected_c_fh_tp, OperationType::ControllerMessageFromHandleToPool)
            .is_none();
        debug_assert!(_c_fh_tp);
        let _c_fe_tp = self.operation_mapping
            .insert(selected_c_fe_tp, OperationType::ControllerMessageFromEpollToPool)
            .is_none();
        debug_assert!(_c_fe_tp);
        let _c_hsr = self.operation_mapping
            .insert(selected_c_hsr, OperationType::ControllerHandleStateReceive)
            .is_none();
        debug_assert!(_c_hsr);
        let _c_sr = self.operation_mapping
            .insert(selected_c_sr, OperationType::ControllerStateReceive)
            .is_none();
        debug_assert!(_c_sr);
        let _kat = self.operation_mapping
            .insert(selected_kat, OperationType::ControllerKeepAliveTicking)
            .is_none();
        debug_assert!(_kat);
    }

    fn initialize_resend_selector<'a>(
        &mut self,
        current_resends: &'a Vec<(PayloadNode, PayloadNode, Receiver<Instant>)>,
        selector: &mut Select<'a>
    ) {
        for resend in current_resends.iter().enumerate() {
            let (receiver_index, (_, _, recv)) = resend;
            let selected = selector.recv(recv);
            let _selected = self.operation_mapping
                .insert(selected, OperationType::UdpResend { receiver_index })
                .is_none();
            debug_assert!(_selected);
        }
    }
}

#[derive(Debug)]
enum OperationType {
    // Emit broadcast periodically
    ControllerBroadcastTicking,

    // Emit keep alive packet periodically
    StandaloneKeepAliveTicking,
    ControllerKeepAliveTicking,

    // Receive messages from the handle, used to send it to the network
    StandaloneMessageFromHandleToPool,
    ControllerMessageFromHandleToPool,

    // Receive messages from the network and send them to the appropriate handle
    StandaloneMessageFromEpollToPool,
    ControllerMessageFromEpollToPool,

    // Handles state updates, received from epoll
    StandaloneHandleStateReceive,
    ControllerHandleStateReceive,

    // Controller state updates, received from the handle (local controller)
    ControllerStateReceive,

    // Special operation used to resend lost packet
    UdpResend { receiver_index: usize }
}