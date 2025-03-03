use std::cmp::Ordering;
use driver_rust::elevio::elev as e;

pub(super) enum State {
    DoorOpen(u8),
    DoorClose(u8),
    Between(u8, u8)
}

impl State {
    pub(super) fn get_current_floor(&self) -> u8 {
        match self {
            State::DoorOpen(floor)
            | State::DoorClose(floor)
            | State::Between(floor, _) => *floor
        }
    }

    pub(super) fn get_direction_to(&self, to: u8) -> u8 {
        match self.get_current_floor().cmp(&to) {
            Ordering::Less => e::DIRN_UP,
            Ordering::Equal => e::DIRN_STOP,
            Ordering::Greater => e::DIRN_DOWN
        }
    }

    pub(super) fn get_direction_from_to(from: u8, to: u8) -> u8 {
        State::Between(from, to).get_direction()
    }
    
    pub(super) fn get_direction(&self) -> u8 {
        match self {
            State::DoorOpen(_) | State::DoorClose(_) => e::DIRN_STOP,
            State::Between(from, to) => {
                match from.cmp(&to) {
                    Ordering::Less => e::DIRN_UP,
                    Ordering::Equal => e::DIRN_STOP,
                    Ordering::Greater => e::DIRN_DOWN
                }
            }
        }
    }
}