use crate::queue::queue::Queue;
use crate::queue::queue_element::Request;
use crate::single_elevator_controller::cabin_state::State;
use driver_rust::elevio::elev as e;
use driver_rust::elevio::elev::Elevator;
use std::cmp::PartialEq;
use driver_rust::elevio::poll::CallButton;
use crate::single_elevator_controller::door_control::DoorControl;

pub struct ElevatorState {
    elevator: Elevator,
    main_queue: Queue,
    door_control: DoorControl,
    current_service: CurrentService,
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
        let request = match call.call {
            e::CAB => Request::Cab(call.floor),
            e::HALL_DOWN => Request::Hall(call.floor, e::DIRN_DOWN),
            e::HALL_UP => Request::Hall(call.floor, e::DIRN_UP),
            _ => panic!("Unexpected State")
        };
        let light_id = request.light_id();

        if self.add_call(request) {
            self.elevator.call_button_light(call.floor, light_id, true);
            self.update_elevator();
        }
    }

    pub fn handle_floor_sensor(&mut self, current_floor: u8) {
        // Check if elevator needs to stop at this floor.
        if self.current_service.does_stop(current_floor) {
            // First stop the elevator.
            self.elevator.motor_direction(e::DIRN_STOP);

            // Elevator state become  "door open", actually open the door and notify the timer
            self.open_door(current_floor);

            // Check to know if we still need to continue or not.
            if self.current_service.is_final_floor(current_floor) {
                // When finished, we reset current service, next call will be honouring the rest
                self.current_service.reset();

                self.elevator.call_button_light(
                    current_floor,
                    self.current_service.request.as_ref().unwrap().light_id(),
                    false
                );

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

struct CurrentService {
    request: Option<Request>,
    state: State,
    serviceable_request: Vec<Request>
}

impl CurrentService {
    fn from(state: State) -> Self {
        Self {
            request: None,
            state,
            serviceable_request: vec![],
        }
    }

    fn is_init(&self) -> bool {
        self.request.is_some()
    }

    fn reset(&mut self) {
        self.request = None
    }

    fn update_request(&mut self, request: Request) {
        self.request = Some(request);
    }

    fn already_serviceable(&self, new_request: &Request) -> bool {
        self.serviceable_request.contains(new_request)
    }

    fn is_current_request(&self, new_request: &Request) -> bool {
        self.request.clone().unwrap().eq(new_request)
    }

    fn update_serviceable(&mut self, queue: &mut Queue) {
        debug_assert!(self.is_init(), "The current service need to be initialized");
        self.serviceable_request.clear();

        let newly_serviceable = queue.retain(|request: &Request|
            ! self.is_current_request(request) &&
                ! self.already_serviceable(request) &&
                ! self.is_serviceable(request));

        for s_req in newly_serviceable {
            if ! self.already_serviceable(&s_req)
                && ! self.is_current_request(&s_req) {
                self.add_to_serviceable(s_req);
            }
        }
    }

    fn is_serviceable(&self, new_request: &Request) -> bool {
        let current_request = self.request.clone().unwrap();

        debug_assert!(! self.already_serviceable(new_request), "An already serviceable request is not serviceable again");
        debug_assert!(! self.is_current_request(new_request), "The current request is not serviceable again");

        if self.is_final_floor(new_request.target()) {
            return false
        }

        let current_direction = self.state.get_direction_to(current_request.target());
        let current_floor = self.state.get_current_floor();

        let is_new_request_direction_ok = match new_request {
            Request::Cab(_) => true,
            Request::Hall(_, direction) => current_direction.eq(direction)
        };

        is_new_request_direction_ok && match current_direction {
            e::DIRN_UP => current_floor < new_request.target()
                && new_request.target() < current_request.target(),
            e::DIRN_DOWN => current_floor > new_request.target()
                && new_request.target() > current_request.target(),
            e::DIRN_STOP => false,
            _ => panic!("Invalid state")
        }
    }

    fn add_to_serviceable(&mut self, request: Request) {
        debug_assert!(self.is_serviceable(&request), "This request is not serviceable");
        self.serviceable_request.push(request);
    }

    fn does_stop(&self, floor: u8) -> bool {
        let original_request = self.request.clone().unwrap();

        if self.is_final_floor(floor) {
            true
        } else {
            let serviceable_hall_req = Request::Hall(
                floor,
                State::get_direction_from_to(floor, original_request.target())
            );
            let serviceable_call_req = Request::Cab(floor);

            for other_req in self.serviceable_request.iter() {
                if serviceable_call_req.eq(other_req) || serviceable_hall_req.eq(other_req) {
                    return true
                }
            }
            false
        }
    }

    fn is_final_floor(&self, floor: u8) -> bool {
        self.request.clone().unwrap().target() == floor
    }

    fn remove_serviced(&mut self, floor: u8) -> Vec<Request> {
        debug_assert!(!self.is_final_floor(floor), "Cannot removed serviced when the request is the final one");
        debug_assert!(self.does_stop(floor), "Cannot remove serviced request when none were serviced.");

        let original_request = self.request.clone().unwrap();
        let mut serviced = Vec::new();

        let serviceable_hall_req = Request::Hall(
            floor,
            State::get_direction_from_to(floor, original_request.target())
        );
        let serviceable_call_req = Request::Cab(floor);

        let mut i = 0;
        while i < self.serviceable_request.len() {
            let other_req = self.serviceable_request.get(i).unwrap();
            if *other_req == serviceable_call_req || *other_req == serviceable_hall_req {
                serviced.push(self.serviceable_request.swap_remove(i));
            }
            i += 1;
        }
        serviced
    }
}