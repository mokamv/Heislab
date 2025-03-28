use std::cmp::Ordering;
use driver_rust::elevio::elev::MotorDirection;
use driver_rust::elevio::elev::MotorDirection::Up;
use crate::config::N_FLOOR;
use crate::data_structures::cabin_state::CabinState::{Between, DoorOpen, Idle, Init};

/// Represents the current state of the elevator cabin
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CabinState {
    /// Initialization state of the cabin, used to indicate the lack of information.
    /// This state is temporary and should be replaced by either [Idle] or [Between] less than 50ms after
    /// connection to the hardware is achieved.
    Init,
    /// Cabin is idle, i.e. not moving, with its doors closed, waiting for the next task.
    Idle { current_floor: u8 },
    /// Cabin is between two adjacent floors.
    /// It's possible to deduce the direction it is going from this state.
    /// This cabin can be immobile in this state.
    Between { from_floor: u8, to_floor: u8 },
    /// Cabin is not moving, staying at a floor, with its door open.
    /// This state is temporary if obstruction is set to false, else it's indefinitely open until obstruction is set to false.
    DoorOpen { current_floor: u8 },
}

impl Default for CabinState {
    /// Default state of the cabin, i.e. [Init]
    fn default() -> Self {
        Init
    }
}

impl CabinState {
    /// Returns true if the cabin state is currently [DoorOpen], false otherwise.
    /// This can be used to avoid pattern-matching when not required
    pub fn is_door_open(&self) -> bool {
        if let DoorOpen { .. } = *self {
            true
        } else { false }
    }

    /// Returns true if the cabin state is currently [Init], false otherwise.
    /// This can be used to avoid pattern-matching when not required
    pub fn is_init(&self) -> bool {
        if let Init = *self {
            true
        } else { false }
    }

    /// Returns true if the cabin state is currently [Between], false otherwise.
    /// This can be used to avoid pattern-matching when not required
    pub fn is_between(&self) -> bool {
        if let Between { .. } = *self {
            true
        } else { false }
    }

    /// Increments the values inside the [Between] state. The increment is positive if [get_direction](CabinState::get_direction) is
    /// [Up] and negative if [Down].
    ///
    /// This function fails if the state of [self] is not [Between]
    ///
    /// Exemple: ``Between { 0, 1 }`` is going from 0 to 1, i.e. going up, so increment will result in
    /// ``Between { 1, 2 }``
    pub fn increment_between(&mut self) {
        let Between { from_floor, to_floor } = *self else { unreachable!() };

        *self = match Self::get_direction_from_to(from_floor, to_floor) {
            MotorDirection::Down => Between { from_floor: to_floor, to_floor: to_floor - 1 },
            Up => Between { from_floor: to_floor, to_floor: to_floor + 1 },
            MotorDirection::Stop => unreachable!()
        }
    }

    ///
    pub fn get_direction_relative_to(&self, to: u8) -> MotorDirection {
        match self.get_current_floor_relative_to(to).cmp(&to) {
            Ordering::Less => Up,
            Ordering::Equal => MotorDirection::Stop,
            Ordering::Greater => MotorDirection::Down
        }
    }

    /// This function compute the floor, the cabin would have departed from if it was idling.
    ///
    /// For [Idle] and [DoorOpen] the result is obviously the stored floor.
    /// For [Between], the result is the from value it would have when moving in the right direction to the [target]
    ///
    /// To be more precise, if [Between] is already going in the right direction to reach target, then the value
    /// of the `from_floor` field is used, else, between is reversed and we take the new from field, i.e. the old `to_floor` field.
    ///
    /// This function is not available when [self] is [Init].
    pub fn get_current_floor_relative_to(&self, target: u8) -> u8 {
        debug_assert!(target < N_FLOOR);
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

    /// Get the last floor the cabin has been on.
    /// For [DoorOpen] and [Idle], it is the value of the `current_floor` field.
    /// For [Between], it is the value of the `from_floor` field.
    ///
    /// This function is not available when [self] is [Init].
    pub fn get_last_seen_floor(&self) -> u8 {
        match *self {
            DoorOpen { current_floor }
            | Idle { current_floor }
            | Between { from_floor: current_floor, .. } => current_floor,
            Init => unreachable!("Function \"get_last_seen_floor\" should not be called on Init state")
        }
    }

    /// Compute the direction required to go from the first floor argument to the second floor argument.
    pub fn get_direction_from_to(from: u8, to: u8) -> MotorDirection {
        match from.cmp(&to) {
            Ordering::Less => Up,
            Ordering::Equal => MotorDirection::Stop,
            Ordering::Greater => MotorDirection::Down
        }
    }

    /// Get the current direction of the cabin.
    /// For [DoorOpen], [Idle] and [Init], return [Stop]
    /// For [Between], return the value computed by [CabinState::get_direction_from_to] with [Between]
    /// fields as arguments
    pub fn get_direction(&self) -> MotorDirection {
        match *self {
            //TODO CHECK UNREACHABLE INIT
            CabinState::Between { from_floor, to_floor } => Self::get_direction_from_to(from_floor, to_floor),
            CabinState::Init => MotorDirection::Down,
            _ => MotorDirection::Stop,
        }
    }

    /// Convert the [CabinState] into a raw bytes array to use in network related code.
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

    /// Reciprocal function to [encode](CabinState::encode)
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