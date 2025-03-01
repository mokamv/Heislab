use crate::messages::TimedMessage;
use crossbeam_channel::SendError;
use crate::connection::synchronisation::link_error::ErrorKind::DeadHandle;

#[derive(Debug)]
pub struct LinkError {
    kind: ErrorKind
}

impl LinkError {
    pub fn new(kind: ErrorKind) -> LinkError {
        Self { kind }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl From<SendError<TimedMessage>> for LinkError {
    fn from(_value: SendError<TimedMessage>) -> Self {
        Self::new(DeadHandle)
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ErrorKind {
    DeadHandle,
    AlreadyConnected
}