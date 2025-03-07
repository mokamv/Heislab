use common::connection::client_pool::client_pool::{ClientPool, Target};
use common::connection::connection_handle::handle::ConnectionIdentifier;
use common::data_struct::CallRequest;
use common::messages::Message;

pub struct LightControl {
    target: Target,
    message: Message
}

impl LightControl {

    pub(super) fn vec_turn_off_for_from(identifier: ConnectionIdentifier, requests: Vec<CallRequest>) -> Vec<LightControl> {
        requests.iter()
            .map(|request| match request {
                CallRequest::Hall { .. } => LightControl::turn_off_for_all(*request),
                CallRequest::Cab { .. } => LightControl::turn_off_for(identifier, *request)
            })
            .collect()
    }

    pub(super) fn turn_on_for_from(identifier: ConnectionIdentifier, request: CallRequest) -> LightControl {
        match request {
            CallRequest::Hall { .. } => LightControl::turn_on_for_all(request),
            CallRequest::Cab { .. } => LightControl::turn_on_for(identifier, request)
        }
    }

    pub(super) fn send(self, client_pool: &mut ClientPool) {
        client_pool.send(
            self.target,
            self.message
        ).unwrap()
    }

    pub(super) fn turn_on_for_all(request: CallRequest) -> Self {
        Self {
            target: Target::All,
            message: Message::LightControl {
                button: request,
                is_lit: true
            },
        }
    }

    pub(super) fn turn_off_for_all(request: CallRequest) -> Self {
        Self {
            target: Target::All,
            message: Message::LightControl {
                button: request,
                is_lit: false
            },
        }
    }

    pub(super) fn turn_on_for(target: ConnectionIdentifier, request: CallRequest) -> Self {
        Self {
            target: Target::Specific(target),
            message: Message::LightControl {
                button: request,
                is_lit: true
            },
        }
    }

    pub(super) fn turn_off_for(target: ConnectionIdentifier, request: CallRequest) -> Self {
        Self {
            target: Target::Specific(target),
            message: Message::LightControl {
                button: request,
                is_lit: false
            },
        }
    }
}