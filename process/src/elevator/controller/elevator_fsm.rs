use std::ops::{BitAndAssign, BitOrAssign};
use std::time::Instant;
use common::data_struct::{CabinState, CallRequest};
use common::messages::Message;
use driver_rust::elevio::elev::MotorDirection;
use std::vec;
use common::config::N_FLOOR;
use common::connection::event_handle::handle_state::ConnectionIdentifier;


pub struct ElevatorState {
    identifier: ConnectionIdentifier,
    is_connected: bool,
    state: CabinState,
    request_matrix: [[bool; 3]; N_FLOOR as usize], // [ hall up | hall down | cab ]
}

impl ElevatorState {
    // FSM methods
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
    pub(super) fn from(identifier: ConnectionIdentifier) -> Self { // Is this used?
        Self {
            identifier,
            is_connected: false,
            state: CabinState::default(),
            request_matrix: [[false; 3]; N_FLOOR as usize]
        }
    }

    pub fn get_total_floors(&self) -> usize {
        self.request_matrix.len()
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

    pub fn get_state_mut(&self) -> &CabinState {
        &self.state
    }

    pub fn get_request_matrix(&self) -> [[bool; 3]; N_FLOOR as usize] {
        self.request_matrix
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub fn can_receive(&self) -> bool {
        ! self.state.is_door_open()
    }

    pub fn add_request(&mut self, request: CallRequest) {
        match request {
            CallRequest::Hall { floor, direction } => {
                match direction {
                    MotorDirection::Up => self.request_matrix[floor as usize][0] = true,
                    MotorDirection::Down => self.request_matrix[floor as usize][1] = true,
                    MotorDirection::Stop => unreachable!()
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
            .map(|floor| floor[2]) // Get only cab requests
            .collect::<Vec<bool>>()
            .try_into()
            .unwrap()
    }

    pub fn merge_cab_requests(&mut self, cab_requests: [bool; N_FLOOR as usize]) {
        for (floor, value) in cab_requests.into_iter().enumerate() {
            self.request_matrix
                .get_mut(floor)
                .unwrap()[2].bitor_assign(value);
        }
    }

    pub fn update_request_matrix(&mut self, request_matrix: [[bool; 3]; N_FLOOR as usize]) {
        self.request_matrix = request_matrix;
    }

    //  TODO: Check if correct
    // Get next floor to visit based on current requests
    pub fn get_next_command(&self) -> Option<Message> {
        // let current_floor = self.state.get_last_seen_floor();
        //
        // // Check if there are any requests in the current floor
        // if self.request_matrix[current_floor as usize].iter().any(|&request| request) {
        //     return Some(Message::GotoFloor { go_to_floor: current_floor });
        // }
        //
        // match self.state.get_direction() {
        //     MotorDirection::Stop => {
        //         // Calculate the closest floor with requests
        //         let mut closest_floor = None;
        //         let mut min_distance = usize::MAX;
        //
        //         for (floor, requests) in self.request_matrix.iter().enumerate() {
        //             if requests.iter().any(|&request| request) {
        //                 let distance = (floor as i32 - current_floor as i32).abs() as usize;
        //                 if distance < min_distance {
        //                     min_distance = distance;
        //                     closest_floor = Some(floor);
        //                 }
        //             }
        //         }
        //         closest_floor.map(|floor| Message::GotoFloor { go_to_floor: floor as u8 })
        //     }
        //     direction => {
        //         let range_same_dir = if direction == MotorDirection::Up {
        //             current_floor + 1..self.get_total_floors()
        //         } else {
        //             0..current_floor
        //         };
        //
        //         // First check for requests in current direction
        //         for floor in range_same_dir {
        //             if self.request_matrix[floor].iter().any(|&request| request) {
        //                 return Some(Message::GotoFloor { go_to_floor: floor as u8 });
        //             }
        //         }
        //         // If no requests in current direction, check opposite direction
        //         let range_opposite = if direction == MotorDirection::Up {
        //             (0..current_floor).rev()
        //         } else {
        //             current_floor + 1..self.get_total_floors()
        //         };
        //
        //         for floor in range_opposite {
        //             if self.request_matrix[floor].iter().any(|&request| request) {
        //                 return Some(Message::GotoFloor { go_to_floor: floor as u8 });
        //             }
        //         }
        //         None
        //     }
        // }
        None
    }

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