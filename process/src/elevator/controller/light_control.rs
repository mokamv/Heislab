use driver_rust::elevio::elev::MotorDirection;
use common::connection::event_handle::controller_handle::controller_handle::Target;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::data_struct::CallRequest;
use common::messages::Message;

pub struct LightControl {
    target: Target,
    message: Message
}

impl LightControl {

    pub(super) fn vec_turn_off_for_from(identifier: ConnectionIdentifier, floor: u8, matrix: &[bool; 3]) -> Vec<LightControl> {
        let mut lights = Vec::new();

        // Check for each possible type of request at floor and turn off lights accordingly
        if matrix[0] {  // hall up
            lights.push(LightControl::turn_off_for_all(CallRequest::Hall {
                floor,
                direction: MotorDirection::Up
            }));
        }
        if matrix[1] {  // hall down
            lights.push(LightControl::turn_off_for_all(CallRequest::Hall {
                floor,
                direction: MotorDirection::Down
            }));
        }
        if matrix[2] {  // cab
            lights.push(LightControl::turn_off_for(identifier, CallRequest::Cab {
                floor
            }));
        }

        lights
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