#[derive(Debug)]
pub struct ClientPoolError {
    error_kind: ErrorKind,
}

impl ClientPoolError {
    pub fn new(error_kind: ErrorKind) -> Self {
        Self {
            error_kind
        }
    }
    
    pub fn kind(&self) -> ErrorKind {
        self.error_kind
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ErrorKind {
    // Send related
    ClientIsDisconnected,
    NoConnectedClient,
    
    BadIdentifier,
    DeadHandle,

    AlreadyConnected
}