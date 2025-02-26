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
    pub fn begin_loop(addr: &str, floor_count: u8) {
        let mut elevator_controller =
            ElevatorController::new(addr, floor_count);

        println!("Elevator started");
        loop {
            cbc::select! {
                recv(elevator_controller.event_channel.call_button_rx) -> a => {
                    let call_button = a.unwrap();
                    elevator_controller.state.handle_call_button(call_button);
                },
                recv(elevator_controller.event_channel.floor_sensor_rx) -> a => {
                    let floor = a.unwrap();
                    elevator_controller.state.handle_floor_sensor(floor)
                },
                recv(elevator_controller.event_channel.stop_button_rx) -> a => {
                    let is_pressed = a.unwrap();
                    elevator_controller.state.handle_stop(is_pressed);
                },
                recv(elevator_controller.event_channel.obstruction_rx) -> a => {
                    let is_obstructed = a.unwrap();
                    elevator_controller.state.handle_obstruction(is_obstructed);
                },
                recv(elevator_controller.event_channel.close_door_rx) -> _ => {
                    elevator_controller.state.handle_close_door();
                }
            }
        }
    }

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

struct EventChannel {
    call_button_rx: Receiver<CallButton>,
    close_door_rx: Receiver<()>,
    floor_sensor_rx: Receiver<u8>,
    stop_button_rx: Receiver<bool>,
    obstruction_rx: Receiver<bool>
}

impl EventChannel {
    fn new(elevator: Elevator, close_door_rx: Receiver<()>) -> EventChannel {
        let poll_period = Duration::from_millis(25);

        let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::call_buttons(elevator, call_button_tx, poll_period));
        }

        let (floor_sensor_tx, floor_sensor_rx) = cbc::unbounded::<u8>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::floor_sensor(elevator, floor_sensor_tx, poll_period));
        }

        let (stop_button_tx, stop_button_rx) = cbc::unbounded::<bool>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::stop_button(elevator, stop_button_tx, poll_period));
        }

        let (obstruction_tx, obstruction_rx) = cbc::unbounded::<bool>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::obstruction(elevator, obstruction_tx, poll_period));
        }

        EventChannel {
            close_door_rx,
            call_button_rx,
            floor_sensor_rx,
            stop_button_rx,
            obstruction_rx,
        }
    }
}