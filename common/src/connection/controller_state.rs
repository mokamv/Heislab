use crate::connection::controller_state::ControllerState::{BACKUP, MASTER};

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ControllerState {
    BACKUP,
    MASTER
}

impl From<u8> for ControllerState {
    fn from(value: u8) -> Self {
        match value {
            u8::MIN => MASTER,
            _ => BACKUP
        }
    }
}

impl Into<u8> for ControllerState {
    fn into(self) -> u8 {
        match self {
            BACKUP => u8::MAX,
            MASTER => u8::MIN
        }
    }
}
