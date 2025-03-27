use common::connection::event_handle::controller_handle;
use common::connection::event_handle::controller_handle::controller_handle::ControllerHandle;
use common::data_struct::{CabinState, CallRequest};
use common::messages::Message;
use driver_rust::elevio::elev::MotorDirection;
use std::vec;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::connection::event_handle::controller_handle::controller_handle::Target;

const N_FLOOR: usize = 4; //TODO: Move to config file
const N_BTN: usize = 3; //TODO: Move to config file

const HALL_UP_IDX: usize = 0; //TODO: Use these?
const HALL_DOWN_IDX: usize = 1;
const CAB_IDX: usize = 2;

pub struct ElevatorState {
    identifier: ConnectionIdentifier,
    is_connected: bool,
    is_obstructed: bool,
    last_direction: MotorDirection, // Last non-stop direction
    state: CabinState,
    request_matrix: Vec<[bool; N_BTN]>, // [ hall up | hall down | cab ]
}

impl ElevatorState {
    // FSM methods
    pub fn fsm_on_request_button_press(&self, controller_handle: &ControllerHandle) {
        let next_floor = self.get_next_command();

        if next_floor.is_none() { // No requests
            return;
        }

        controller_handle.send_client_message(
            Target::Specific(self.identifier),
            Message::GotoFloor { go_to_floor: next_floor.unwrap() }
        );
        
    }

    // Light control methods

    pub fn set_all_lights(&self, controller_handle: &ControllerHandle) {
        for f in 0..N_FLOOR {
            for btn in 0..N_BTN {
                let request = match btn {
                    0 => CallRequest::Hall { 
                        floor: f as u8, 
                        direction: MotorDirection::Up 
                    },
                    1 => CallRequest::Hall { 
                        floor: f as u8, 
                        direction: MotorDirection::Down 
                    },
                    2 => CallRequest::Cab { 
                        floor: f as u8 
                    },
                    _ => unreachable!()
                };
    
                controller_handle.send_client_message(
                    Target::Specitic(self.identifier),
                    Message::LightControl { 
                        button: request,
                        is_lit: self.request_matrix[f][btn]
                    }
                );
            }
        }
    }


    //TODO
    // pub fn on_init_floor_arrival(&self, floor: u8) -> Self {
    //     match self.state {
    //         CabinState::Init => CabinState::Idle { current_floor: floor },
    //         _ => *self.state
    //     }
    // }
    //
    // pub fn on_floor_arrival(&self, floor: u8, has_requests: bool) -> Self {
    //     match self.state {
    //         CabinState::Between { to_floor, .. } if *to_floor == floor && has_requests => {
    //             CabinState::DoorOpen { current_floor: floor }
    //         }
    //         CabinState::Between { .. } => {
    //             CabinState::Idle { current_floor: floor }
    //         }
    //         _ => *self.state
    //     }
    //     self.get_next_command();
    // }
}


impl ElevatorState {
    // Request functions:
    pub fn requests_above(&self, floor: u8) -> bool {
        if floor as usize >= N_FLOOR-1{
            return false;
        }

        for f in floor as usize + 1..N_FLOOR {
            for btn in 0..N_BTN {
                if self.request_matrix[f][btn] {
                    return true;
                }
            }
        }
        false
    }

    pub fn requests_below(&self, floor: u8) -> bool {
        if floor == 0 {
            return false;
        }

        for f in 0..floor as usize {
            for btn in 0..N_BTN {
                if self.request_matrix[f][btn] {
                    return true;
                }
            }
        }
        false
    }

    pub fn requests_at_current_floor(&self, floor: u8) -> bool {
        for btn in 0..N_BTN {
            if self.request_matrix[floor as usize][btn] {
                return true;
            }
        }
        false
    }

    pub fn choose_direction(&self , current_floor: u8, direction: MotorDirection) -> MotorDirection { //TODO: Check if correct
        match direction {
            MotorDirection::Up => {
                if self.requests_above(current_floor) {
                    MotorDirection::Up }
                else if self.requests_below(current_floor) {
                    MotorDirection::Down
                } else {
                    MotorDirection::Stop
                }
            }
            MotorDirection::Down => {
                if self.requests_below(current_floor) {
                    MotorDirection::Down
                } else if self.requests_above(current_floor) {
                    MotorDirection::Up
                } else {
                    MotorDirection::Stop
                }
            }
            MotorDirection::Stop => {
                if self.requests_above(current_floor) {
                    MotorDirection::Up
                } else if self.requests_below(current_floor) {
                    MotorDirection::Down
                } else {
                    MotorDirection::Stop
                }
            }
        }
    }

    pub fn check_should_stop(&self) -> bool {
        let current_floor = self.state.get_last_seen_floor();
        let direction = self.last_direction;

        if self.request_matrix[current_floor as usize][2] {
            return true;
        }

        match direction {
            MotorDirection::Up => {
                return {
                    self.request_matrix[current_floor as usize][0] || // Hall up
                    self.request_matrix[current_floor as usize][2] || // Cab
                    !self.requests_above(current_floor)
                }
            }
            MotorDirection::Down => {
                return {
                    self.request_matrix[current_floor as usize][1] || // Hall down
                    self.request_matrix[current_floor as usize][2] || // Cab
                    !self.requests_below(current_floor)
                }
            }
            MotorDirection::Stop => {}
        }
        true
    }

    pub fn check_should_clear_request_immediately(&self, floor: u8, request: CallRequest) -> bool {
        self.request_matrix[floor as usize][2]
    }

}

impl ElevatorState {
    pub(super) fn from(identifier: ConnectionIdentifier) -> Self { // Is this used?
        Self {
            identifier,
            is_connected: false,
            is_obstructed: false,
            last_direction: MotorDirection::Stop,
            state: CabinState::default(),
            request_matrix: vec![[false; N_BTN]; N_FLOOR]
        }
    }

    pub(super) fn identifier(&self) -> ConnectionIdentifier {
        self.identifier
    }

    pub fn set_connected(&mut self, is_connected: bool) {
        self.is_connected = is_connected;
    }

    pub fn set_state(&mut self, state: CabinState) {
        self.state = state;
    }

    pub fn get_state(&self) -> &CabinState {
        &self.state
    }

    pub fn get_state_mut(&mut self) -> &mut CabinState {
        &mut self.state
    }

    pub fn get_request_matrix(&self) -> &Vec<[bool; N_BTN]> {
        &self.request_matrix
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub fn can_receive(&self) -> bool {
        !self.state.is_door_open()
    }

    pub fn add_request(&mut self, request: CallRequest) { // TODO: Error if request is not valid
        match request {
            CallRequest::Hall { floor, direction } => {
                match direction {
                    MotorDirection::Up => self.request_matrix[floor as usize][0] = true,
                    MotorDirection::Down => self.request_matrix[floor as usize][1] = true,
                    MotorDirection::Stop => {}
                }
            }
            CallRequest::Cab { floor } => {
                self.request_matrix[floor as usize][2] = true;
            }
        }
    }

    // Clear hall requests
    pub fn clear_hall_requests(&mut self) {
        for floor_requests in self.request_matrix.iter_mut() {
            floor_requests[0] = false; // Clear hall up
            floor_requests[1] = false; // Clear hall down
        }
    }

    // Get hall requests in the format needed for the hall request assigner
    pub fn get_hall_requests(&self) -> Vec<[bool; 2]> {
        self.request_matrix
            .iter()
            .map(|floor| [floor[0], floor[1]]) // Convert [hall_up, hall_down, cab] to [hall_up, hall_down]
            .collect()
    }

    // Get cab requests in the format needed for the hall request assigner
    pub fn get_cab_requests(&self) -> Vec<bool> {
        self.request_matrix
            .iter()
            .map(|floor| floor[2]) // Get only cab requests
            .collect()
    }

    pub fn update_request_matrix(&mut self, request_matrix: Vec<[bool; N_BTN]>) {
        self.request_matrix = request_matrix;
    }

    pub fn get_next_command(&self) -> Option<u8> {
        let current_floor = self.state.get_last_seen_floor();

        if self.requests_at_current_floor(current_floor) {
            return Some(current_floor);
        }

        let next_direction = self.choose_direction(
            current_floor,
            self.last_direction
        );


        match next_direction {
            MotorDirection::Stop => None, // No requests
            
            MotorDirection::Up => {
                // Check for requests above current floor
                for floor in current_floor + 1..N_FLOOR {
                    if self.requests_at_current_floor(floor) {
                        return Some(floor);
                    }
                }
                // If no requests above, check below (change direction)
                if self.requests_below(current_floor) {
                    for floor in (0..current_floor).rev() {
                        if self.requests_at_current_floor(floor) {
                            return Some(floor);
                        }
                    }
                }
                None
            },
            
            MotorDirection::Down => {
                // Check for requests below current floor
                for floor in (0..current_floor).rev() {
                    if self.requests_at_current_floor(floor) {
                        return Some(floor);
                    }
                }
                // If no requests below, check above (change direction)
                if self.requests_above(current_floor) {
                    for floor in current_floor + 1..N_FLOOR {
                        if self.requests_at_current_floor(floor) {
                            return Some(floor);
                        }
                    }
                }
                None
            }
        }

    }
    //  TODO: Check if correct
    // Get next floor to visit based on current requests

    // Clear requests for a specific floor
    //TODO
    // pub fn complete_request_at_floor(&mut self, reached_floor: u8) -> Vec<LightControl> {
    //     let floor = reached_floor as usize;
    //     let current_requests = self.request_matrix[floor];
    //
    //     // Handle lights
    //     let lights = LightControl::vec_turn_off_for_from(
    //         self.identifier,
    //         reached_floor,
    //         &current_requests
    //     );
    //
    //     // Clear all requests for this floor
    //     self.request_matrix[floor] = [false; 3];
    //
    //     lights
    // }
}