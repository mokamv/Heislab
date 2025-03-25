use crate::connection::constants::UDP_TIMEOUT;
use crate::connection::event_handle::controller_handle::controller_handle::ControllerHandleState;
use crate::connection::event_handle::handle_state::{ConnectionIdentifier, HandleState, HANDLE_ACK_UNINIT, HANDLE_HASH_UNINIT};
use crate::messages::TimedPayload;
use crossbeam_channel::Sender;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::Instant;

pub(in super::super::super) struct ControllerHandleEpollChannels {
    controller_id: ConnectionIdentifier,

    // Storing sync state
    sync_state: RefCell<HandleState>,
    // Storing clients_state
    clients_state: RefCell<HashMap<ConnectionIdentifier, HandleState>>,

    controller_state_sender: Sender<ControllerHandleState>,
    to_pool_from_epoll: Sender<TimedPayload>,
}

impl ControllerHandleEpollChannels {
    pub(in super) fn new(
        controller_id: ConnectionIdentifier,
        client_ids: HashSet<ConnectionIdentifier>,
        handle_state_sender: Sender<ControllerHandleState>,
        to_pool_from_epoll: Sender<TimedPayload>
    ) -> Self {
        let now = Instant::now();

        Self {
            controller_id,
            sync_state: RefCell::new(Default::default()),
            clients_state: RefCell::new(
                client_ids
                    .iter()
                    .map(|client_id| (
                        *client_id,
                        HandleState::Disconnected {
                            since: now,
                        }

                    ))
                    .collect()
            ),
            controller_state_sender: handle_state_sender,
            to_pool_from_epoll,
        }
    }

    pub(in super::super::super) fn get_controller_id(&self) -> ConnectionIdentifier {
        self.controller_id
    }

    pub(in super::super::super) fn check_timeout(&self) {
        match self.get_sync_handle_state() {
            HandleState::Connected { last_update, .. } =>
                if last_update.elapsed() > UDP_TIMEOUT {
                    self.send_sync_disconnect();
                },
            HandleState::Disconnected { .. } => {}
        }

        let clients_id: Vec<ConnectionIdentifier> = self
            .clients_state
            .borrow()
            .keys()
            .map(|x| *x)
            .collect();

        for client_id in clients_id {
            let client_handle = self.get_client_handle_state(client_id);
            match client_handle {
                HandleState::Connected { last_update, .. } =>
                    if last_update.elapsed() > UDP_TIMEOUT {
                        self.send_client_disconnect(client_id);
                    },
                HandleState::Disconnected { .. } => {}
            }
        }
    }

    pub(in super::super::super) fn send_sync_message_to_pool(&self, message: TimedPayload) {
        let borrowed_state = self
            .sync_state
            .borrow();
        if let HandleState::Connected { hash, .. } = *borrowed_state {
            if message.payload().hash() == hash || message.payload().message().is_ack() {
                let _ = self
                    .to_pool_from_epoll
                    .send(message)
                    .unwrap();
            }
        }
    }

    pub(in super::super::super) fn get_sync_handle_state(&self) -> HandleState {
        *self.sync_state
            .borrow()
    }

    pub(in super::super::super) fn update_sync_connected_state(
        &self,
        recv_ack: usize,
        recv_hash: usize
    ) {
        let mut borrowed_state_mut = self
            .sync_state
            .borrow_mut();

        match &mut *borrowed_state_mut {
            HandleState::Disconnected { .. } => unreachable!(),
            HandleState::Connected {
                ack,
                hash,
                last_update,
                ..
            } => {
                if *ack == HANDLE_ACK_UNINIT {
                    *ack = recv_ack;
                    *hash = recv_hash;
                    *last_update = Instant::now();

                    drop(borrowed_state_mut);
                    let _ = self.controller_state_sender.send(
                        ControllerHandleState::SyncState {
                            state: *self.sync_state.borrow(),
                        }
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
                        self.send_sync_disconnect()
                    }
                }
            }
        }
    }

    pub(in super::super::super) fn send_sync_disconnect(&self) {
        let new_state = HandleState::Disconnected {
            since: Instant::now()
        };

        *self.sync_state.borrow_mut() = new_state;

        let _ = self.controller_state_sender.send(
            ControllerHandleState::SyncState {
                state: new_state
            }
        ).unwrap();
    }

    pub(in super::super::super) fn send_sync_connect(
        &self,
        connected_to: ConnectionIdentifier,
        address: SocketAddr
    ) {
        let new_state = HandleState::Connected {
            connected_to,
            address,
            last_update: Instant::now(),
            since: Instant::now(),
            ack: HANDLE_ACK_UNINIT,
            hash: HANDLE_HASH_UNINIT,
        };

        *self.sync_state.borrow_mut() = new_state;

        let _ = self.controller_state_sender.send(
            ControllerHandleState::SyncState {
                state: new_state
            }
        ).unwrap();
    }

    pub(in super::super::super) fn send_client_message_to_pool(
        &self,
        client_id: ConnectionIdentifier,
        message: TimedPayload
    ) {
        let borrowed_state = self
            .clients_state
            .borrow();
        if let HandleState::Connected {
            hash,
            ..
        } = *borrowed_state.get(&client_id).unwrap() {
            if message.payload().hash() == hash || message.payload().message().is_ack() {
                let _ = self
                    .to_pool_from_epoll
                    .send(message)
                    .unwrap();

            }
        }
    }

    pub(in super::super::super) fn get_client_handle_state(&self, client_id: ConnectionIdentifier) -> HandleState {
        *self.clients_state
            .borrow()
            .get(&client_id)
            .unwrap()
    }

    pub(in super::super::super) fn update_client_connected_state(
        &self,
        client_id: ConnectionIdentifier,
        recv_ack: usize,
        recv_hash: usize
    ) {
        let mut borrowed_state_mut = self
            .clients_state
            .borrow_mut();

        match &mut *borrowed_state_mut.get_mut(&client_id).unwrap() {
            HandleState::Disconnected { .. } => unreachable!(),
            HandleState::Connected {
                ack,
                hash,
                last_update,
                ..
            } => {
                // print!("CURRENT ACK: {ack} RECEIVED HASH: {recv_ack}");
                // println!("CURRENT HASH: {hash} RECEIVED HASH: {recv_hash}");

                //Newer keep_alive payload, ignore older ones.
                if recv_ack > *ack {
                    // Same hash, just update the timeout
                    if recv_hash == *hash {
                        *ack = recv_ack;
                        *last_update = Instant::now();
                    }
                    // hashes differ, means new connection (last one has been dropped or lost)
                    else {
                        drop(borrowed_state_mut);
                        self.send_client_disconnect(client_id)
                    }
                }
            }
        }
    }

    pub(in super::super::super) fn send_client_disconnect(&self, client_id: ConnectionIdentifier) {
        let new_state = HandleState::Disconnected {
            since: Instant::now()
        };
        *self.clients_state
            .borrow_mut()
            .get_mut(&client_id)
            .unwrap() = new_state;

        let _ = self.controller_state_sender.send(
            ControllerHandleState::ClientState {
                client_id,
                state: new_state
            }
        ).unwrap();
    }

    pub(in super::super::super) fn send_client_connect(
        &self,
        client_id: ConnectionIdentifier,
        address: SocketAddr,
        ack: usize,
        hash: usize
    ) {
        let new_state = HandleState::Connected {
            connected_to: client_id,
            address,
            last_update: Instant::now(),
            since: Instant::now(),
            ack,
            hash,
        };
        *self.clients_state
            .borrow_mut()
            .get_mut(&client_id)
            .unwrap() = new_state;

        let _ = self.controller_state_sender.send(
            ControllerHandleState::ClientState {
                client_id,
                state: new_state
            }
        ).unwrap();
    }
}