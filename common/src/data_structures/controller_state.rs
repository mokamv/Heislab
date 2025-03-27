use crate::data_structures::controller_state::ControllerState::{Backup, Master, MasterSteppingDown};

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ControllerState {
    /// Currently acting as backup, receiving controller_link message from the current master
    Backup,
    /// Currently a master in the process of being downgraded to a backup, this is the state during reconciliation
    MasterSteppingDown,
    /// Currently a master, handles clients and keeps the backup synchronised.
    Master,
}

impl From<u8> for ControllerState {
    /// Convert an unsigned integer used with network code to a [ControllerState].
    fn from(value: u8) -> Self {
        match value {
            u8::MIN => Master,
            u8::MAX => Backup,
            _ => MasterSteppingDown
        }
    }
}

impl Into<u8> for ControllerState {
    /// Convert a [ControllerState] into an unsigned integer usable with network code.
    fn into(self) -> u8 {
        match self {
            Backup => u8::MAX,
            MasterSteppingDown => 127,
            Master => u8::MIN
        }
    }
}