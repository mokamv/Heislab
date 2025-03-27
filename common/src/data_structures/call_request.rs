use driver_rust::elevio::elev::{CallType, MotorDirection};
use driver_rust::elevio::elev::MotorDirection::Up;
use crate::data_structures::call_request::CallRequest::{Cab, Hall};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CallRequest {
    Hall { floor: u8, direction: MotorDirection },
    Cab { floor: u8 }
}

impl Into<CallType> for CallRequest {
    fn into(self) -> CallType {
        match self {
            Cab { .. } => CallType::Cab,
            Hall { direction, .. } => match direction {
                MotorDirection::Down => CallType::HallDown,
                Up => CallType::HallUp,
                MotorDirection::Stop => unreachable!()
            }
        }
    }
}

impl CallRequest {
    pub(crate) fn encode(&self) -> [u8; 3] {
        let mut message = [0u8; 3];
        match self {
            Hall { floor, direction } => {
                message[0] = 0;
                message[1] = *floor;
                message[2] = *direction as u8;
            }
            Cab { floor } => {
                message[0] = 1;
                message[1] = *floor;

            }
        };
        message
    }

    pub(super) fn decode(raw_button: &[u8]) -> Self {
        assert_eq!(raw_button.len(), 3);
        match raw_button[0] {
            0 => Hall { floor: raw_button[1], direction: raw_button[2].try_into().unwrap() },
            _ => Cab { floor: raw_button[1] }
        }
    }

    /// This method returns the 'target' of the request.
    ///
    /// A `target` is the floor the cabin needs to reach to complete the call
    ///
    /// A Hall button `target` is the floor the button sits at.
    ///
    /// A Cab button `target` is the value associated to the button.
    pub fn target(&self) -> u8 {
        match self { Hall { floor, .. } | Cab { floor } => *floor }
    }

    /// This method return the `direction` of the request, if it has one.
    ///
    /// Only [Hall](Request::Hall) requests have a `direction`.
    pub fn direction(&self) -> Option<MotorDirection> {
        match *self {
            Cab {..} => None,
            Hall { direction, .. } => Some(direction)
        }
    }

    /// This method returns the `light_id` associated with the request.
    ///
    /// A `light_id` is used to turn of and on specific call light on the elevator control panel.
    pub fn light_id(&self) -> u8 {
        match *self {
            Cab { .. } => CallType::Cab as u8,
            Hall { direction, .. } => match direction {
                Up => CallType::HallUp as u8,
                MotorDirection::Down => CallType::HallDown as u8,
                _ => unreachable!("Shouldn't happen")
            }
        }
    }
}