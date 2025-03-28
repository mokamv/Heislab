use common::config::N_FLOOR;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::constants::{CAB_IDX, HALL_DOWN_IDX, HALL_UP_IDX, N_BUTTONS};
use common::data_structures::cabin_state::CabinState;
use common::data_structures::call_request::CallRequest;
use driver_rust::elevio::elev::MotorDirection;
use std::ops::BitOrAssign;

#[derive(Debug)]
pub struct ElevatorState {
    identifier: ConnectionIdentifier,
    is_connected: bool,
    is_obstructed: bool,
    is_motor_locked: bool,
    last_direction: MotorDirection, // Last non-stop direction
    state: CabinState,
    request_matrix: [[bool; N_BUTTONS]; N_FLOOR as usize], // [ hall up | hall down | cab ]
}

impl ElevatorState {
    // Request functions:

    /// Return true if there are requests above last registered floor, false otherwise.
    pub fn requests_above(&self) -> bool {
        let floor = self.state.get_last_seen_floor();
        for f in floor + 1..N_FLOOR {
            for btn in 0..N_BUTTONS {
                if self.request_matrix[f as usize][btn] {
                    return true;
                }
            }
        }
        false
    }

    /// Return true if there are requests below last registered floor, false otherwise.
    pub fn requests_below(&self) -> bool {
        let floor = self.state.get_last_seen_floor();
        for f in 0..floor as usize {
            for btn in 0..N_BUTTONS {
                if self.request_matrix[f][btn] {
                    return true;
                }
            }
        }
        false
    }

    /// Return true if there are requests at last registered floor, false otherwise.
    pub fn requests_at_current_floor(&self) -> bool {
        let floor = self.state.get_last_seen_floor();
        for btn in 0..N_BUTTONS {
            if self.request_matrix[floor as usize][btn] {
                return true;
            }
        }
        false
    }


    /// Calculate the next direction based on the current state
    pub fn choose_direction(
        &self,
    ) -> MotorDirection {
        match self.last_direction {
            MotorDirection::Up => {
                // If there are requests above the current floor for the elevator to service,
                // the elevator should be concidered to be going up.
                // If not, change direction to downwards as long as there still are requests
                // last_direction will be set to Stop if there are no more requests
                if self.requests_above() {
                    MotorDirection::Up
                } else if self.requests_at_current_floor() {
                    MotorDirection::Down
                } else if self.requests_below() {
                    MotorDirection::Down
                } else {
                    MotorDirection::Stop
                }
            }
            MotorDirection::Down => {
                // If there are requests below the current floor for the elevator to service,
                // the elevator should be concidered to be going down.
                // If not, change direction to upwards as long as there still are requests
                // The last_direction will be set to Stop if there are no more requests
                if self.requests_below() {
                    MotorDirection::Down
                } else if self.requests_at_current_floor() {
                    MotorDirection::Up
                } else if self.requests_above() {
                    MotorDirection::Up
                } else {
                    MotorDirection::Stop
                }
            }
            MotorDirection::Stop => {
                // If the elevator is Idle, and there are requests up or down, the elevator should
                // start moving in the direction of the requests.
                // If there are no requests, the elevator should remain idle
                if self.requests_at_current_floor() {
                    MotorDirection::Stop
                } else if self.requests_above() {
                    MotorDirection::Up
                } else if self.requests_below() {
                    MotorDirection::Down
                } else {
                    MotorDirection::Stop
                }
            }
        }
    }
}

impl ElevatorState {

    pub(super) fn from(identifier: ConnectionIdentifier) -> Self {
        Self {
            identifier,
            is_connected: false,
            is_obstructed: false,
            is_motor_locked: false,
            last_direction: MotorDirection::Stop,
            state: Default::default(),
            request_matrix: [[false; N_BUTTONS]; N_FLOOR as usize]
        }
    }

    pub(super) fn get_elevator_identifier(&self) -> ConnectionIdentifier {
        self.identifier
    }

    pub fn set_connected(&mut self, is_connected: bool) {
        self.is_connected = is_connected;
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub fn set_obstructed(&mut self, is_obstructed: bool) {
        self.is_obstructed = is_obstructed;
    }

    pub fn is_obstructed(&self) -> bool {
        self.is_obstructed
    }

    pub fn set_motor_locked(&mut self, is_motor_locked: bool) {
        self.is_motor_locked = is_motor_locked;
    }

    pub fn is_motor_locked(&self) -> bool {
        self.is_motor_locked
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

    pub fn get_last_direction(&self) -> MotorDirection {
        self.last_direction
    }

    pub fn get_request_matrix(&self) -> [[bool; N_BUTTONS]; N_FLOOR as usize] {
        self.request_matrix
    }

    /// Add a request to the elevator state
    pub fn add_request(&mut self, request: CallRequest) {
        // Add the request to the request matrix
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

    pub fn remove_request(&mut self, request: CallRequest) {
        match request {
            CallRequest::Hall { floor, direction } => {
                match direction {
                    MotorDirection::Up => self.request_matrix[floor as usize][HALL_UP_IDX] = false,
                    MotorDirection::Down => self.request_matrix[floor as usize][HALL_DOWN_IDX] = false,
                    MotorDirection::Stop => unreachable!()
                }
            }
            CallRequest::Cab { floor } => {
                self.request_matrix[floor as usize][CAB_IDX] = false;
            }
        }
    }

    pub fn on_door_open_clear_cab_request(&mut self) {
        // Clear the cab request
        debug_assert!(self.state.is_door_open()); // The elevator must be in the DoorOpen state
        self.request_matrix[self.state.get_last_seen_floor() as usize][CAB_IDX] = false;
    }

    /// Called when an elevator reach the state [DoorOpen](CabinState::DoorOpen)
    /// Clear the relevant hall requests if possible and return the associated [CallRequest]
    /// The return value can be used to clear lights for example.
    pub fn on_door_open_clear_relevant_hall_requests(&mut self) -> Vec<CallRequest> {
        debug_assert!(self.state.is_door_open());
        let floor = self.state.get_last_seen_floor();

        let mut cleared = vec![];

        match self.last_direction {
            MotorDirection::Stop => {
                let request_up = &mut self.request_matrix[floor as usize][HALL_UP_IDX];
                if *request_up {
                    *request_up = false;
                    cleared.push(CallRequest::Hall { floor, direction: MotorDirection::Up });
                }

                let request_down = &mut self.request_matrix[floor as usize][HALL_DOWN_IDX];
                if *request_down {
                    *request_down = false;
                    cleared.push(CallRequest::Hall { floor, direction: MotorDirection::Down });
                }
            }

            MotorDirection::Down => {
                if ! self.requests_below() && self.request_matrix[floor as usize][HALL_UP_IDX] {
                    self.request_matrix[floor as usize][HALL_UP_IDX] = false;
                    cleared.push(CallRequest::Hall { floor, direction: MotorDirection::Up });
                }
                if self.request_matrix[floor as usize][HALL_DOWN_IDX] {
                    self.request_matrix[floor as usize][HALL_DOWN_IDX] = false;
                    cleared.push(CallRequest::Hall { floor, direction: MotorDirection::Down });
                }
            },

            MotorDirection::Up => {
                if ! self.requests_above() && self.request_matrix[floor as usize][HALL_DOWN_IDX] {
                    self.request_matrix[floor as usize][HALL_DOWN_IDX] = false;
                    cleared.push(CallRequest::Hall { floor, direction: MotorDirection::Down });
                }
                if self.request_matrix[floor as usize][HALL_UP_IDX] {
                    self.request_matrix[floor as usize][HALL_UP_IDX] = false;
                    cleared.push(CallRequest::Hall { floor, direction: MotorDirection::Up });
                }
            }
        }

        cleared
    }

    pub fn clear_hall_requests(&mut self) {
        // clear all hall requests in the elevators request matrix
        for floor_requests in self.request_matrix.iter_mut() {
            floor_requests[HALL_UP_IDX] = false; // Clear hall up
            floor_requests[HALL_DOWN_IDX] = false; // Clear hall down
        }
    }

    /// Get hall requests in the format needed for the hall request assigner
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

    /// Reassign cab requests to the elevator when synchronizing
    pub fn replace_cab_requests(&mut self, cab_requests: [bool; N_FLOOR as usize]) {
        // Replace current cab requests in the request matrix
        self.request_matrix
            .iter_mut()
            .enumerate()
            .for_each(|(floor, floor_array)| {
                floor_array[CAB_IDX] = cab_requests[floor]
            })
    }

    /// Merge cab requests to the elevator when synchronizing
    pub fn merge_cab_requests(&mut self, cab_requests: [bool; N_FLOOR as usize]) {
        // Iter through the request matrix and merge the cab requests
        // Keeping current cab requests if they are true and adding new ones
        self.request_matrix
            .iter_mut()
            .enumerate()
            .for_each(|(floor, floor_array)| {
                floor_array[CAB_IDX].bitor_assign(cab_requests[floor]) // 
            });
    }

    /// Merge hall requests to the elevator when synchronizing
    pub fn merge_request_matrix(&mut self, request_matrix: [[bool; N_BUTTONS]; N_FLOOR as usize]) {
        // Merge input request matrix with the current request matrix
        // Keeping current requests if they are true and adding new ones
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

    /// Calculates the next command for the elevator based on the current state and requests assigned to the elevator
    /// Returns the next floor to visit or None if the elevator should remain idle
    pub fn get_next_command(&mut self) -> Option<u8> {
        // If the elevator is in the Init or DoorOpen state, it should not service new request yet
        if self.state.is_init() || self.state.is_door_open() {
            return None;
        }

        let floor = self.state.get_last_seen_floor();
        let is_between_floor = self.state.is_between();

        // Get the current direction of the elevator
        self.last_direction = self.choose_direction();

        match self.last_direction {
            // When direction is stop, i.e. not going to move
            MotorDirection::Stop => {
                // It can mean that there are only calls on current floor
                if self.requests_at_current_floor() {
                    Some(floor)
                }
                // Or that there are no more calls
                else {
                    None
                }
            }
            // When direction is down
            MotorDirection::Down => {
                let floor = if is_between_floor { floor - 1 } else { floor };
                let mut consider = None;

                // Check if there are any requests below the current floor in the order from closest to furthest from current floor
                for f in (0..=floor).rev() {
                    // Check if there are any HALL_DOWN or CAB requests that are fitted to be serviced in the current direction
                    if self.request_matrix[f as usize][HALL_DOWN_IDX]
                        || self.request_matrix[f as usize][CAB_IDX] {
                        return Some(f);
                    // If not, check if there are any HALL_UP requests that are fitted to be serviced in the current direction
                    } else if self.request_matrix[f as usize][HALL_UP_IDX] {
                        consider = Some(f);
                    }
                }
                consider
            }
            // When direction is up
            MotorDirection::Up => {
                let floor = if is_between_floor { floor + 1 } else { floor };
                let mut consider = None;

                // Check if there are any requests above the current floor in the order from closest to furthest from current floor
                for f in floor..N_FLOOR {
                    // Check if there are any HALL_UP or CAB requests that are fitted to be serviced in the current direction
                    if self.request_matrix[f as usize][HALL_UP_IDX]
                        || self.request_matrix[f as usize][CAB_IDX] {
                        return Some(f);
                    // If not, check if there are any HALL_DOWN requests that are fitted to be serviced in the current direction
                    } else if self.request_matrix[f as usize][HALL_DOWN_IDX] {
                        consider = Some(f);
                    }
                }
                consider
            }
        }
    }
}