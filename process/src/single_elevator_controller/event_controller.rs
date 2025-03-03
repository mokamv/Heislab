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

    fn cost(state_vector: &mut Vec<ElevatorState>, elevators_running: u8, call: CallButton) -> u8{
        let weight_vec: Vec<i8> = vec![0; elevators_running as usize]; //max weight 127
        incr = 0;

        for state in state_vector.iter_mut() {
            if ! state.is_running(){ //TODO: make a state struct that holds which elevators are working
                weight_vec[incr] = 127
            }
            else if ! state.add_call(call){ //allreaddy in queue
                weight_vec[incr] = -128
                break;
            }
            else if state.current_service.state.get_direction_from_to(){
                weight_vec[incr] = -1
                break;
                //TODO: check if the call is on the way
                //TODO: check if right function
            }
            else {
                weight_vec[incr] = state.main_queue.len() 
            }
            incr += 1;
        }
        return min of weight_vec index

        min_index = 0;
        min_value = 200;
        incr = 0
        for weight in weight_vec.iter_mut();
            if weight < min_value{
                min_index = 0;
            }
            incr += 1;

        min_index

    }
}

