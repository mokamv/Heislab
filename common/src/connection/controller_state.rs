use std::sync::{Arc, Mutex};
use crossbeam_channel::{unbounded, Receiver, Sender};
use crate::connection::connection_handle::handle::ConnectionIdentifier;
use crate::connection::controller_state::ControllerState::{Backup, Master, MasterSteppingDown};
use crate::log::log_client::ReliableLogSender;
use crate::log::LogLevel;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ControllerState {
    /// Currently acting as backup, receiving synchronisation message from the current master
    Backup,
    /// Currently a master in the process of being downgraded to a backup, this is the state during reconciliation
    MasterSteppingDown,
    /// Currently a master, handles clients and synchronise every backup.
    Master,
}

impl From<u8> for ControllerState {
    fn from(value: u8) -> Self {
        match value {
            u8::MIN => Master,
            u8::MAX => Backup,
            _ => MasterSteppingDown
        }
    }
}

impl Into<u8> for ControllerState {
    fn into(self) -> u8 {
        match self {
            Backup => u8::MAX,
            MasterSteppingDown => 127,
            Master => u8::MIN
        }
    }
}

pub struct ControllerStateNotifier {
    state_sender: Sender<ControllerState>,
    logger: ReliableLogSender,
    current_state: Arc<Mutex<ControllerState>>,
    controller_id: ConnectionIdentifier
}

impl Clone for ControllerStateNotifier {
    fn clone(&self) -> Self {
        ControllerStateNotifier {
            state_sender: self.state_sender.clone(),
            logger: self.logger.clone(),
            current_state: self.current_state.clone(),
            controller_id: self.controller_id
        }
    }
}

impl ControllerStateNotifier {
    pub(super) fn new(controller_id: ConnectionIdentifier, logger: ReliableLogSender) -> (Self, Receiver<ControllerState>) {
        let (state_sender, state_receiver) = unbounded();

        (Self {
            state_sender,
            logger,
            current_state: Arc::new(Mutex::new(Backup)),
            controller_id
        }, state_receiver)
    }

    pub(super) fn change_state(&self, state: ControllerState) {
        let mut current_connect_status = self.current_state.lock().unwrap();
        if *current_connect_status != state {
            *current_connect_status = state;
            drop(current_connect_status);
            self.log_new_state(state);
            if let Err(_) = self.state_sender.send(state) {
                println!("FAILED"); //TODO ERROR
            }
        }
    }

    pub(super) fn current_state(&self) -> ControllerState {
        *self.current_state.lock().unwrap()
    }

    pub(super) fn controller_id(&self) -> ConnectionIdentifier {
        self.controller_id
    }

    fn log_new_state(&self, state: ControllerState) {
        match state {
            Backup => self.logger.send("Becoming backup after reconciliation.", LogLevel::INFO),
            MasterSteppingDown => self.logger.send("Stepping down from the role of master for reconciliation.", LogLevel::INFO),
            Master => self.logger.send("Becoming master since no other master is alive or responding.", LogLevel::INFO)
        }
    }
}