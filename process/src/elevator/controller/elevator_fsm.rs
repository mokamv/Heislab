use common::config::N_FLOOR;
use common::connection::event_handle::controller_handle::controller_handle::Target;
use common::connection::event_handle::controller_handle::controller_handle::ControllerHandle;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::constants::{CAB_IDX, HALL_DOWN_IDX, HALL_UP_IDX, N_BUTTONS};
use driver_rust::elevio::elev::MotorDirection;
use std::ops::BitOrAssign;
use common::data_structures::cabin_state::CabinState;
use common::data_structures::call_request::CallRequest;
use common::data_structures::network::message::Message;

#[derive(Debug)]
pub struct ElevatorState {
    identifier: ConnectionIdentifier,
    is_connected: bool,
    is_obstructed: bool,
    last_direction: MotorDirection, // Last non-stop direction
    state: CabinState,
    request_matrix: [[bool; N_BUTTONS]; N_FLOOR as usize], // [ hall up | hall down | cab ]
}

impl ElevatorState {
    // FSM methods
    pub fn fsm_on_request_button_press(&mut self, controller_handle: &ControllerHandle) {
        let next_floor = self.get_next_command();

        if next_floor.is_none() { // No requests
            return;
        }

        let next_floor = next_floor.unwrap();
        let current_floor = self.state.get_last_seen_floor();
        
        // Update current_direction based on next floor
        if next_floor > current_floor {
            self.last_direction = MotorDirection::Up;
        } else if next_floor < current_floor {
            self.last_direction = MotorDirection::Down;
        }

        controller_handle.send_client_message(
            Target::Specific(self.identifier),
            Message::GotoFloor { go_to_floor: next_floor }
        );

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
        if floor >= N_FLOOR  - 1{
            return false;
        }

        for f in floor + 1..N_FLOOR {
            for btn in 0..N_BUTTONS {
                if self.request_matrix[f as usize][btn] {
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
            for btn in 0..N_BUTTONS {
                if self.request_matrix[f][btn] {
                    return true;
                }
            }
        }
        false
    }

    pub fn requests_at_current_floor(&self, floor: u8) -> bool {
        for btn in 0..N_BUTTONS {
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

        if self.request_matrix[current_floor as usize][CAB_IDX] {
            return true;
        }

        match direction {
            MotorDirection::Up => {
                return {
                    self.request_matrix[current_floor as usize][HALL_UP_IDX] || // Hall up
                    self.request_matrix[current_floor as usize][CAB_IDX] || // Cab
                    !self.requests_above(current_floor)
                }
            }
            MotorDirection::Down => {
                return {
                    self.request_matrix[current_floor as usize][HALL_DOWN_IDX] || // Hall down
                    self.request_matrix[current_floor as usize][CAB_IDX] || // Cab
                    !self.requests_below(current_floor)
                }
            }
            MotorDirection::Stop => {}
        }
        true
    }

    pub fn check_should_clear_request_immediately(&self, floor: u8, request: CallRequest) -> bool {
        self.request_matrix[floor as usize][CAB_IDX]
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
            request_matrix: [[false; N_BUTTONS]; N_FLOOR as usize]
        }
    }

    pub(super) fn identifier(&self) -> ConnectionIdentifier {
        self.identifier
    }

    pub fn set_connected(&mut self, is_connected: bool) {
        self.is_connected = is_connected;
    }

    pub fn set_state(&mut self, state: CabinState) {
        // Store direction before going idle
        if matches!(state, CabinState::Idle { .. }) {
            // Keep the current_direction as is - it will be used to determine
            // which direction to resume when new requests arrive
        }
        else {
            self.last_direction = match state {
                CabinState::Between { .. } => state.get_direction(),
                _ => self.last_direction
            };
        };
        
        self.state = state;
    }

    pub fn get_state(&self) -> &CabinState {
        &self.state
    }

    pub fn get_state_mut(&mut self) -> &mut CabinState {
        &mut self.state
    }

    pub fn get_last_direction(&self) -> MotorDirection {
        self.last_direction
    }

    pub fn get_request_matrix(&self) -> [[bool; N_BUTTONS]; N_FLOOR as usize] {
        self.request_matrix
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub fn can_receive(&self) -> bool {
        !self.state.is_door_open()
    }

    pub fn add_request(&mut self, request: CallRequest) {
        match request {
            CallRequest::Hall { floor, direction } => {
                match direction {
                    MotorDirection::Up => self.request_matrix[floor as usize][HALL_UP_IDX] = true,
                    MotorDirection::Down => self.request_matrix[floor as usize][HALL_DOWN_IDX] = true,
                    MotorDirection::Stop => unreachable!()
                }
            }
            CallRequest::Cab { floor } => {
                self.request_matrix[floor as usize][CAB_IDX] = true;
            }
        }
    }

    // Clear hall requests at a specific floor, and specific direction
    pub fn clear_relevant_hall_requests_at_floor(&mut self, floor: u8, direction: MotorDirection) {
        match direction {
            MotorDirection::Up => self.request_matrix[floor as usize][HALL_UP_IDX] = false,
            MotorDirection::Down => self.request_matrix[floor as usize][HALL_DOWN_IDX] = false,
            MotorDirection::Stop => unreachable!()
        }
    }

    pub fn clear_cab_requests_at_floor(&mut self, floor: u8) {
        self.request_matrix[floor as usize][CAB_IDX] = false;
    }

    // Clear hall requests
    pub fn clear_hall_requests(&mut self) {
        for floor_requests in self.request_matrix.iter_mut() {
            floor_requests[HALL_UP_IDX] = false; // Clear hall up
            floor_requests[HALL_DOWN_IDX] = false; // Clear hall down
        }
    }

    // Get hall requests in the format needed for the hall request assigner
    pub fn get_hall_requests(&self) -> [[bool; 2]; N_FLOOR as usize] {
        self.request_matrix
            .iter()
            .map(|floor| [floor[0], floor[1]]) // Convert [hall_up, hall_down, cab] to [hall_up, hall_down]
            .collect::<Vec<[bool; 2]>>()
            .try_into()
            .unwrap()
    }

    // Get cab requests in the format needed for the hall request assigner
    pub fn get_cab_requests(&self) -> [bool; N_FLOOR as usize] {
        self.request_matrix
            .iter()
            .map(|floor| floor[CAB_IDX]) // Get only cab requests
            .collect::<Vec<bool>>()
            .try_into()
            .unwrap()
    }

    pub fn merge_cab_requests(&mut self, cab_requests: [bool; N_FLOOR as usize]) {
        for (floor, value) in cab_requests.into_iter().enumerate() {
            self.request_matrix
                .get_mut(floor)
                .unwrap()[CAB_IDX].bitor_assign(value);
        }
    }

    pub fn merge_request_matrix(&mut self, request_matrix: [[bool; N_BUTTONS]; N_FLOOR as usize]) {
        self.request_matrix
            .iter_mut()
            .enumerate()
            .for_each(|(floor, floor_array)| {
                floor_array.into_iter()
                    .zip(request_matrix[floor].into_iter())
                    .for_each(|(floor_value, new_value)| {
                        floor_value.bitor_assign(new_value);
                    })
            });
    }

    pub fn replace_request_matrix(&mut self, request_matrix: [[bool; N_BUTTONS]; N_FLOOR as usize]) {
        self.request_matrix = request_matrix;
    }

    pub fn get_next_command(&self) -> Option<u8> {
        // If elevator is in init state, between floors or door is open, return None
        if self.state.is_init() || self.state.is_door_open() {
            return None;
        }

        // Get current floor
        let current_floor = self.state.get_last_seen_floor();

        if self.request_matrix[current_floor as usize][CAB_IDX] {
            return Some(current_floor);
        }


        match self.last_direction {
            MotorDirection::Stop => {
                let last_floor = self.state.get_last_seen_floor();
                let distance: usize = usize::MAX;
                let mut closest_floor: Option<u8> = None;

                for floor in 0..N_FLOOR as u8 {
                    if self.requests_at_current_floor(floor) {
                        let new_distance = (floor as i32 - last_floor as i32).abs() as usize;
                        if new_distance < distance {
                            closest_floor = Some(floor);
                        }
                    }
                }
                closest_floor
            },
            MotorDirection::Up => {
                // Check for requests above current floor
                for floor in current_floor + 1..N_FLOOR as u8 {
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
                None // No requests
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
                    for floor in current_floor + 1..N_FLOOR as u8 {
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