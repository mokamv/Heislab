use std::cell::RefCell;
use crate::connection::event_handle::handle_state::{ConnectionIdentifier, HandleState, HANDLE_ACK_UNINIT, HANDLE_HASH_UNINIT};
use crate::messages::TimedPayload;
use crossbeam_channel::Sender;
use std::net::SocketAddr;
use std::time::Instant;
use crate::connection::constants::UDP_TIMEOUT;

pub(in super::super::super) struct StandaloneHandleEpollChannels {
    connection_id: ConnectionIdentifier,
    standalone_state: RefCell<HandleState>,
    handle_state_sender: Sender<HandleState>,
    to_pool_from_epoll: Sender<TimedPayload>
}

impl StandaloneHandleEpollChannels {
    pub(in super) fn new(
        connection_id: ConnectionIdentifier,
        handle_state_sender: Sender<HandleState>,
        to_pool_from_epoll: Sender<TimedPayload>
    ) -> Self {
        Self {
            connection_id,
            standalone_state: Default::default(),
            handle_state_sender,
            to_pool_from_epoll
        }
    }

    pub(in super::super::super) fn send_message(
        &self,
        message: TimedPayload,
        controller_id: ConnectionIdentifier
    ) {
        let borrowed_state = self
            .standalone_state
            .borrow();
        if let HandleState::Connected {
            hash,
            connected_to,
            ..
        } = *borrowed_state {
            if connected_to == controller_id {
                if message.payload().hash() == hash ||
                    message.payload().message().is_ack() {
                    let _ = self
                        .to_pool_from_epoll
                        .send(message)
                        .unwrap();
                }
            }
        }
    }

    pub(in super::super::super) fn get_handle_state(&self) -> HandleState {
        *self.standalone_state.borrow()
    }

    pub(in super::super::super) fn update_connected_state(
        &self,
        controller_id: ConnectionIdentifier,
        recv_ack: usize,
        recv_hash: usize
    ) {
        let mut borrowed_state_mut = self.standalone_state.borrow_mut();

        match &mut *borrowed_state_mut {
            HandleState::Disconnected { .. } => unreachable!(),
            HandleState::Connected {
                connected_to,
                ack,
                hash,
                last_update,
                ..
            } => {
                if *connected_to != controller_id {
                    return;
                }

                if *ack == HANDLE_ACK_UNINIT {
                    *ack = recv_ack;
                    *hash = recv_hash;
                    *last_update = Instant::now();

                    drop(borrowed_state_mut);
                    let _ = self.handle_state_sender.send(
                        *self.standalone_state.borrow()
                    ).unwrap();
                }
                //Newer keep_alive payload, ignore older ones.
                else if recv_ack > *ack {
                    // Same hash, just update the timeout
                    if recv_hash == *hash {
                        *ack = recv_ack;
                        *last_update = Instant::now();
                    }
                    // hashes differ, means new connection (last one has been dropped or lost)
                    else {
                        drop(borrowed_state_mut);
                        self.send_disconnect()
                    }
                }
            }
        }
    }

    pub(in super::super::super) fn check_timeout(&self) {
        match self.get_handle_state() {
            HandleState::Connected { last_update, .. } =>
                if last_update.elapsed() > UDP_TIMEOUT {
                    self.send_disconnect();
                },
            HandleState::Disconnected { .. } => {}
        }
    }

    pub(in super::super::super) fn send_disconnect(&self) {
        let new_state = HandleState::Disconnected {
            since: Instant::now()
        };

        *self.standalone_state.borrow_mut() = new_state;
        self.handle_state_sender.send(new_state).unwrap()
    }

    pub(in super::super::super) fn send_connect(
        &self,
        controller_id: ConnectionIdentifier,
        address: SocketAddr,
    ) {
        // println!("CONNECTING TO {controller_id} at {address}");

        let new_state = HandleState::Connected {
            connected_to: controller_id,
            address,
            since: Instant::now(),
            last_update: Instant::now(),
            ack: HANDLE_ACK_UNINIT,
            hash: HANDLE_HASH_UNINIT,
        };

        *self.standalone_state.borrow_mut() = new_state;
        self.handle_state_sender.send(new_state).unwrap()
    }

    // TODO REMOVE (ONLY USED IN DEBUG ASSERT)
    pub(in super::super::super) fn get_connection_id(&self) -> ConnectionIdentifier {
        self.connection_id
    }
}