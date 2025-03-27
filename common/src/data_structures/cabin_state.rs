use std::cmp::Ordering;
use driver_rust::elevio::elev::MotorDirection;
use driver_rust::elevio::elev::MotorDirection::Up;
use crate::data_structures::cabin_state::CabinState::{Between, DoorOpen, Idle, Init};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CabinState {
    Init,
    Idle { current_floor: u8 },
    Between { from_floor: u8, to_floor: u8 }, // Moving between to floors
    DoorOpen { current_floor: u8 },
}

impl Default for CabinState { // is this used?
    fn default() -> Self {
        Init
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
            Up => Between { from_floor: to_floor, to_floor: to_floor + 1 },
            MotorDirection::Stop => unreachable!()
        }
    }

    pub fn get_direction_relative_to(&self, to: u8) -> MotorDirection {
        match self.get_current_floor_relative_to(to).cmp(&to) {
            Ordering::Less => Up,
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
            Ordering::Less => Up,
            Ordering::Equal => MotorDirection::Stop,
            Ordering::Greater => MotorDirection::Down
        }
    }

    pub fn get_direction(&self) -> MotorDirection {
        match *self {
            CabinState::Between { from_floor, to_floor } => Self::get_direction_from_to(from_floor, to_floor),
            CabinState::Init => MotorDirection::Down,
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