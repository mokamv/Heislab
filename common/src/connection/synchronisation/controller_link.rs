use crate::connection::connection_handle::handle::{ConnectionHandle, ConnectionIdentifier};
use crate::connection::synchronisation::link_error::ErrorKind::AlreadyConnected;
use crate::connection::synchronisation::link_error::LinkError;
use crate::messages::Message::Authenticated;
use crate::messages::{Message, TimedMessage};
use crossbeam_channel::{Receiver, Sender};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use log::log_client::ReliableLogSender;
use crate::connection::client_pool::client_pool::ClientPool;
use crate::connection::connection_handle::message_sender::MessageSender;
use crate::connection::controller_state::ControllerStateNotifier;

pub struct ControllerLink {
    handle: Arc<Mutex<ConnectionHandle>>,
    internal_sender: Sender<TimedMessage>
}

impl ControllerLink {
    pub(crate) fn take_sender(&self) -> MessageSender {
        self.handle.lock().unwrap().take_sender()
    }
}

impl ControllerLink {
    pub(crate) fn take_receiver(&self) -> Receiver<Message> {
        self.handle.lock().unwrap().take_receiver()
    }
}

impl Clone for ControllerLink {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            internal_sender: self.internal_sender.clone()
        }
    }
}

impl ControllerLink {
    pub(crate) fn new(
        controller_state_notifier: ControllerStateNotifier,
        client_pool: ClientPool,
        logger: ReliableLogSender
    ) -> Self {

        let backup_handle = ConnectionHandle::new_backup_connection_handler(
            controller_state_notifier,
            client_pool,
            logger
        );

        Self {
            internal_sender: backup_handle.borrow_sender().clone(),
            handle: Arc::new(Mutex::new(backup_handle))
        }
    }

    pub(in super::super) fn connect_stream(
        &self,
        controller_id: ConnectionIdentifier,
        controller_stream: TcpStream
    ) -> Result<(), LinkError> {

        let handle = self.handle.lock().unwrap();

        // Check for already connected instance, this is an error
        if handle.is_connected() {
            let _ = controller_stream.shutdown(Shutdown::Both);
            Err(LinkError::new(AlreadyConnected))
        } else {
            handle.connect_to(controller_stream, controller_id, true);
            if let Err(error) = self.internal_sender.send(TimedMessage::of(Authenticated)) {
                handle.disconnect();
                Err(error.into())
            } else { Ok(()) }
        }
    }
}