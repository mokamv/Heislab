use crate::queue::queue::Queue;
use crate::queue::queue_element::Request;
use crate::single_elevator_controller::cabin_state::State;
use driver_rust::elevio::elev as e;
use driver_rust::elevio::elev::Elevator;
use std::cmp::PartialEq;
use driver_rust::elevio::poll::CallButton;
use crate::single_elevator_controller::door_control::DoorControl;
use crate::process::client::ClientState;
use common::message::Message;

pub struct ElevatorState {
    // elevator: Elevator,
    main_queue: Queue,
    // door_control: DoorControl,
    current_service: CurrentService,
    is_connected: bool
}

impl ElevatorState {
    pub fn new(current_floor: u8, door_control: DoorControl, elevator: Elevator) -> ElevatorState {
        let mut calibrated = ElevatorState {
            elevator,
            main_queue: Queue::new(8),
            door_control,
            current_service: CurrentService::from(State::DoorClose(current_floor))
        };

        calibrated.add_call(Request::Cab(current_floor));

        calibrated
    }

    pub fn new_uncalibrated(door_control: DoorControl, elevator: Elevator) -> ElevatorState {
        let mut uncalibrated = Self::new(0, door_control, elevator);
        uncalibrated.current_service.state = State::Between(u8::MAX, 0);

        uncalibrated
    }

    pub fn handle_obstruction(&self, is_obstructed: bool) {
        self.door_control.obstruction(is_obstructed)
    }

    pub fn handle_stop(&self, is_pressed: bool) {
        if is_pressed {
            for i in 0..3 {
                for j in 0..self.elevator.num_floors {
                    self.elevator.call_button_light(j, i , false);
                }
            }
        }
    }

    pub fn handle_close_door(&mut self) {
        if let State::DoorOpen(current_floor) = self.current_service.state {
            // We close the door.
            self.elevator.door_light(false);
            self.current_service.state = State::DoorClose(current_floor);
            self.update_elevator();
        } else {
            panic!("Invalid State")
        }
    }

    pub fn handle_call_button(&mut self, call: CallButton) {
        //TODO: run the cost function?
        
        let light_id = request.light_id();
        let cost = self.cost(request); 

        //TODO: send message with cost to the event controller
        Message::ElevatorCost { cost: cost };
        ClientState.message_sender.send(TimedMessage::of(Message::ElevatorCost { client_id: self.identifier })).unwrap();


        // TODO: First check cost before adding to queue

        // if self.add_call(request) {
        //     self.elevator.call_button_light(call.floor, light_id, true);
        //     self.update_elevator();
        // }
    }

    pub fn handle_floor_sensor(&mut self, current_floor: u8) {
        self.elevator.floor_indicator(current_floor);
        // Check if elevator needs to stop at this floor.
        if self.current_service.does_stop(current_floor) {
            // First stop the elevator.
            self.elevator.motor_direction(e::DIRN_STOP);

            // Elevator state become  "door open", actually open the door and notify the timer
            self.open_door(current_floor);

            // Check to know if we still need to continue or not.
            if self.current_service.is_final_floor(current_floor) {
                // When finished, we reset current service, next call will be honouring the rest
                self.elevator.call_button_light(
                    current_floor,
                    self.current_service.request.as_ref().unwrap().light_id(),
                    false
                );
                
                self.current_service.reset();

                // We still need to remove potential hall light that'll be serviced right after.
                if let Some(next_request) = self.main_queue.peek() {
                    self.elevator.call_button_light(current_floor, next_request.light_id(), false);
                }
            } else {
                let serviced_requests = self.current_service.remove_serviced(current_floor);
                for s_req in serviced_requests {
                    self.elevator.call_button_light(current_floor, s_req.light_id(), false);
                }
            }
        } else {
            // No need to stop, just updating the state to be accurate.
            //TODO TAKE A LOOK AT THIS.
            self.current_service.state =
                State::Between(
                    current_floor,
                     self.current_service.request.as_ref().unwrap().target()
                )
        }
    }

    fn open_door(&mut self, current_floor: u8) {
        self.current_service.state = State::DoorOpen(current_floor);
        self.door_control.open();
        self.elevator.door_light(true);
    }

    fn add_call(&mut self, request: Request) -> bool {
        //TODO HANDLE PRIO (Hall down mean cab, prio to cab )

        if ! self.current_service.is_init() {
            if self.main_queue.is_empty() {
                // There are no element remaining and the elevator current servicce isn't initialized
                // This means we directly update the current service request then trigger elevator logic
                self.current_service.update_request(request);
                return true
            }
        } else {
            if self.current_service.already_serviceable(&request)
                || self.current_service.is_current_request(&request) {
                return false
            } else if self.current_service.is_serviceable(&request) {
                self.current_service.add_to_serviceable(request);
                return true
            }
        }
        self.main_queue.push_unique(request)
    }

    fn update_elevator(&mut self) {
        match self.current_service.state {
            // Nothing to be done in those states.
            State::DoorOpen(_) | State::Between(_, _) => {}

            // State can only be updated in this specific state, that is waiting for action.
            State::DoorClose(current_floor) => {

                if self.current_service.is_init() {
                    let current_request = self.current_service.request.clone().unwrap();

                    // If the current task is to be there, just open the door and clear the light
                    if current_request.target() == current_floor {
                        self.current_service.reset();

                        self.open_door(current_floor);

                        self.elevator.call_button_light(
                            current_floor,
                            current_request.light_id(),
                            false
                        );
                    } else {
                        self.current_service.state =
                            State::Between(
                                current_floor,
                                current_request.target()
                            );
                        self.elevator.motor_direction(self.current_service.state.get_direction());
                    }
                } else if ! self.main_queue.is_empty() {
                    self.current_service.update_request(self.main_queue.pop().unwrap());
                    self.current_service.update_serviceable(&mut self.main_queue);
                    self.update_elevator();
                }
            }
        }
    }
}

