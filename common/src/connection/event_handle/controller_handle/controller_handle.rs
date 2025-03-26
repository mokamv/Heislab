use crate::connection::event_handle::controller_handle::controller_handle_epoll_channels::ControllerHandleEpollChannels;
use crate::connection::event_handle::controller_handle::controller_handle_pool_channels::ControllerHandlePoolChannels;
use crate::connection::event_handle::handle_state::{ConnectionIdentifier, HandleState};
use crate::messages::{Message, TimedMessage};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::HashSet;
use crate::data_struct::ControllerState;

#[derive(Debug, Copy, Clone)]
pub enum Target {
    All,
    Specific(ConnectionIdentifier)
}

#[derive(Debug)]
pub(in super::super) enum SentControllerMessage {
    Sync { message: TimedMessage },
    Client { target: Target, message: TimedMessage }
}

#[derive(Debug)]
pub(in super::super) enum  ControllerHandleState {
    ClientState { client_id: ConnectionIdentifier, state: HandleState } ,
    SyncState { state: HandleState }
}

#[derive(Debug)]
pub struct ClientMessage {
    pub client_id: ConnectionIdentifier,
    pub message: Message
}

pub(in super::super) struct ControllerHandleBuilder {
    pool_channels: Option<ControllerHandlePoolChannels>,
    epoll_channels: Option<ControllerHandleEpollChannels>,
    handle: ControllerHandle
}

impl ControllerHandleBuilder {
    pub(in super::super) fn new(configuration: ControllerHandleConfiguration) -> Self {
        let (handle_state_sender, handle_state_receiver) = unbounded();
        let (controller_state_sender, controller_state_receiver) = unbounded();

        let (to_pool_from_epoll, from_epoll_to_pool) = unbounded();

        let (to_handle_from_client_pool, from_client_pool_to_handle) = unbounded();
        let (to_handle_from_sync, from_sync_to_handle) = unbounded();
        let (to_pool_from_handle, from_handle_to_pool, ) = unbounded();

        ControllerHandleBuilder {
            pool_channels: Some(
                ControllerHandlePoolChannels::new(
                    configuration.controller_id,
                    configuration.client_ids.clone(),
                    handle_state_receiver,
                    controller_state_receiver,
                    from_epoll_to_pool,
                    to_handle_from_client_pool,
                    to_handle_from_sync,
                    from_handle_to_pool
                )
            ),
            epoll_channels: Some(
                ControllerHandleEpollChannels::new(
                    configuration.controller_id,
                    configuration.client_ids.clone(),
                    handle_state_sender,
                    to_pool_from_epoll
                )
            ),
            handle: ControllerHandle {
                clients_id: configuration
                    .client_ids
                    .into_iter()
                    .collect(),
                from_client_pool_to_handle,
                from_sync_to_handle,
                to_pool_from_handle,
                controller_state_sender
            },
        }
    }

    pub(in super::super) fn take_controller_handle_epoll_channels(&mut self) -> ControllerHandleEpollChannels {
        self.epoll_channels.take()
            .expect("This can only be done once.")
    }

    pub(in super::super) fn take_controller_handle_pool_channels(&mut self) -> ControllerHandlePoolChannels {
        self.pool_channels.take()
            .expect("This can only be done once.")
    }

    pub(in super::super) fn into_controller_handle(self) -> ControllerHandle {
        self.handle
    }
}

pub struct ControllerHandleConfiguration {
    client_ids: HashSet<ConnectionIdentifier>,
    controller_id: ConnectionIdentifier
}

impl ControllerHandleConfiguration {
    pub fn new(controller_id: ConnectionIdentifier) -> Self {
        Self {
            client_ids: Default::default(),
            controller_id,
        }
    }

    pub fn add_client(&mut self, client_id: ConnectionIdentifier) -> &mut Self {
        let inserted = self.client_ids.insert(client_id);
        if ! inserted {
            panic!("Cannot use the same id twice")
        }
        self
    }
}

pub struct ControllerHandle {
    // Store ids of every possible clients
    clients_id: Vec<ConnectionIdentifier>,

    // Receive messages from both the other controller(sync) and the connected clients
    from_client_pool_to_handle: Receiver<ClientMessage>,
    from_sync_to_handle: Receiver<Message>,

    // Send messages to one or all of the connected clients
    // Send messages to the other controller
    to_pool_from_handle: Sender<SentControllerMessage>,

    // Send updated controller state to the pool
    controller_state_sender: Sender<ControllerState>
}

impl ControllerHandle {
    pub fn clients_id(&self) -> &[ConnectionIdentifier] {
        &self.clients_id[..]
    }

    pub fn recv_sync_message(&self) -> &Receiver<Message> {
        &self.from_sync_to_handle
    }

    pub fn recv_client_message(&self) -> &Receiver<ClientMessage> {
        &self.from_client_pool_to_handle
    }

    pub fn send_client_message(&self, target: Target, message: Message) {
        let _ = self.to_pool_from_handle.send(SentControllerMessage::Client {
            target,
            message: TimedMessage::of(message),
        }).unwrap();
    }

    pub fn send_sync_message(&self, message: Message) {
        let _ = self.to_pool_from_handle.send(SentControllerMessage::Sync {
            message: TimedMessage::of(message),
        }).unwrap();
    }

    pub fn send_controller_state(&self, state: ControllerState) {
        let _ = self.controller_state_sender.send(
            state
        ).unwrap();
    }
}