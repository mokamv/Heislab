use crossbeam_channel as cbc;
use driver_rust::elevio;
use driver_rust::elevio::elev::Elevator;
use std::thread::spawn;
use std::time::Duration;
use crossbeam_channel::Receiver;
use driver_rust::elevio::poll::CallButton;
use crate::single_elevator_controller::door_control::DoorControl;
use crate::single_elevator_controller::elevator_state::ElevatorState;

pub struct ElevatorController {
    state_vector: Vec<ElevatorState>,
    event_channel: EventChannel
}

impl ElevatorController { 
    fn new(addr: &str, floor_count: u8) -> Self {
        let (door_control, close_door_rx) = DoorControl::new();
        let elevator = Elevator::init(addr, floor_count).expect("TODO");

        let elevator_controller = match elevator.floor_sensor() {
            None => {
                elevator.motor_direction(elevio::elev::DIRN_DOWN);
                ElevatorController {
                    state: ElevatorState::new_uncalibrated(door_control, elevator.clone()),
                    event_channel: EventChannel::new(elevator, close_door_rx)
                }
            }
            Some(current_floor) => {
                ElevatorController {
                    state: ElevatorState::new(current_floor, door_control, elevator.clone()),
                    event_channel: EventChannel::new(elevator, close_door_rx)
                }
            }
        };

        elevator_controller
    }
}

