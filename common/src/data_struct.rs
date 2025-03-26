use driver_rust::elevio::elev::{CallType, MotorDirection};
use std::cmp::Ordering;

use crate::data_struct::CabinState::{Between, DoorOpen, Idle, Init};
use crate::data_struct::CallRequest::{Cab, Hall};

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
                MotorDirection::Up => CallType::HallUp,
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
                MotorDirection::Up => CallType::HallUp as u8,
                MotorDirection::Down => CallType::HallDown as u8,
                _ => unreachable!("Shouldn't happen")
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CabinState {
    Idle { current_floor: u8 },
    Between { from_floor: u8, to_floor: u8 }, // Moving between to floors
    DoorOpen { current_floor: u8 },
    Init
}

impl Default for CabinState {
    fn default() -> Self {
        CabinState::Init
    }
}

impl CabinState {
    // FSM methods

    pub fn on_door_timeout(&self) -> Option<CabinState> {
        match self {
            DoorOpen { current_floor } => Some(CabinState::Idle { current_floor: *current_floor }),
            _ => None,
        }
    }

    pub fn on_floor_arrival(&self, floor: u8, has_pending_requests: bool) -> Option<CabinState> {
        match self {
            // If the cabin is between floors and has pending requests, open the door
            Between { .. } if has_pending_requests => Some(DoorOpen { current_floor: floor }),
            // If the cabin is between floors and has no pending requests, stop at the floor
            Between { .. } => Some(Idle { current_floor: floor }),
            _ => None,
        }
    }
}

impl CabinState {
    pub fn is_between(&self) -> bool {
        if let Between { .. } = *self {
            true
        } else { false }
    }

    pub fn is_door_open(&self) -> bool {
        if let DoorOpen { .. } = *self {
            true
        } else { false }
    }

    pub fn is_idle(&self) -> bool {
        if let Idle { .. } = *self {
            true
        } else { false }
    }

    pub fn is_init(&self) -> bool {
        if let Init = *self {
            true
        } else { false }
    }

    pub fn increment_between(&mut self) {
        let Between { from_floor, to_floor } = *self else { unreachable!() };

        *self = match Self::get_direction_from_to(from_floor, to_floor) {
            MotorDirection::Down => Between { from_floor: to_floor, to_floor: to_floor - 1 },
            MotorDirection::Up => Between { from_floor: to_floor, to_floor: to_floor + 1 },
            MotorDirection::Stop => unreachable!()
        }
    }

    pub fn get_direction_relative_to(&self, to: u8) -> MotorDirection {
        match self.get_current_floor_relative_to(to).cmp(&to) {
            Ordering::Less => MotorDirection::Up,
            Ordering::Equal => MotorDirection::Stop,
            Ordering::Greater => MotorDirection::Down
        }
    }

    pub fn get_current_floor_relative_to(&self, target: u8) -> u8 {
        match self {
            Idle { current_floor }
            | DoorOpen { current_floor } => *current_floor,
            Between { from_floor, to_floor } => {
                let from_distance = (target as i32 - *from_floor as i32).abs();
                let to_distance = (target as i32 - *to_floor as i32).abs();

                match from_distance.cmp(&to_distance) {
                    Ordering::Less => *to_floor,
                    Ordering::Equal => unreachable!(),
                    Ordering::Greater => *from_floor
                }
            }
            Init => unreachable!("Function \"get_current_floor_relative_to\" should not be called on Init state")
        }
    }

    pub fn get_last_seen_floor(&self) -> u8 {
        match *self {
            DoorOpen { current_floor }
            | Idle { current_floor }
            | Between { from_floor: current_floor, .. } => current_floor,
            Init => unreachable!("Function \"get_last_seen_floor\" should not be called on Init state")
        }
    }

    pub fn get_direction_from_to(from: u8, to: u8) -> MotorDirection {
        match from.cmp(&to) {
            Ordering::Less => MotorDirection::Up,
            Ordering::Equal => MotorDirection::Stop,
            Ordering::Greater => MotorDirection::Down
        }
    }

    pub fn get_direction(&self) -> MotorDirection {
        match *self {
            Between { from_floor, to_floor } => Self::get_direction_from_to(from_floor, to_floor),
            Init => unreachable!("Function \"get_direction\" should not be called on Init state"),
            _ => MotorDirection::Stop,

        }
    }

    pub(crate) fn encode(&self) -> [u8; 3] {
        let mut message = [0u8; 3];
        match *self {
            DoorOpen { current_floor } => {
                message[0] = 0;
                message[1] = current_floor;
            }
            Idle { current_floor } => {
                message[0] = 1;
                message[1] = current_floor;
            }
            Between { from_floor, to_floor } => {
                message[0] = 2;
                message[1] = from_floor;
                message[2] = to_floor;
            }
            Init => {
                message[0] = 3;
            }
        };
        message
    }

    pub(super) fn decode(raw_cabin_state: &[u8]) -> Self {
        assert_eq!(raw_cabin_state.len(), 3);
        match raw_cabin_state[0] {
            0 => DoorOpen { current_floor: raw_cabin_state[1] },
            1 => Idle { current_floor: raw_cabin_state[1] },
            2 => Between { from_floor: raw_cabin_state[1], to_floor: raw_cabin_state[2] },
            3 => Init,
            _ => unreachable!()
        }
    }

}