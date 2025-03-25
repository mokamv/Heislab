use driver_rust::elevio::elev::MotorDirection;
use crate::data_struct::CabinState::{Between, DoorOpen, Idle, Init};

pub struct Behaviour_pair {
    direction: MotorDirection,
    behaviour: CabinState
};

