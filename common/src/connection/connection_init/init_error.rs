#[derive(Debug)]
pub struct InitError {
    kind: ErrorKind
}

impl InitError {
    pub fn new(error_kind: ErrorKind) -> Self {
        Self {
            kind: error_kind
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

}

#[derive(Copy, Clone, Debug)]
pub enum ErrorKind {
    IdentificationTimedOut,
    UnexpectedMessageType,
    WrongController
}