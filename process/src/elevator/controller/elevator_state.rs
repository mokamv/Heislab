use crate::elevator::controller::elevator_service::ElevatorService;
use crate::elevator::controller::light_control::LightControl;
use common::connection::connection_handle::handle::ConnectionIdentifier;
use common::data_struct::{CabinState, CallRequest};
use common::messages::Message;
use std::collections::VecDeque;
use std::vec;
use driver_rust::elevio::elev::MotorDirection;

const N_FLOOR: usize = 4; //TODO: Move to config file

pub struct ElevatorState {
    identifier: ConnectionIdentifier,
    is_connected: bool,
    state: CabinState,
    request_matrix: Vec<[bool; 3], N_FLOOR>, // [ hall up | hall down | cab ]
}

impl ElevatorState {
    pub(super) fn from(identifier: ConnectionIdentifier) -> Self {
        Self {
            identifier,
            is_connected: false,
            state: Default::default(),
            request_matrix: vec![[false; 3]; N_FLOOR]
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

    pub fn get_request_matrix(&self) -> &Vec<[bool; 3]> {
        &self.request_matrix
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub fn can_receive(&self) -> bool {
        ! self.state.is_door_open()
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

    // Clear hall requests (keep cab requests)
    pub fn clear_hall_requests(&mut self) {
        for floor_requests in self.request_matrix.iter_mut() {
            floor_requests[0] = false; // Clear hall up
            floor_requests[1] = false; // Clear hall down
        }
    }

    // Get hall requests in the format needed for the hall request assigner
    pub fn get_hall_requests(&self) -> Vec<Vec<bool>> {
        self.request_matrix
            .iter()
            .map(|floor| vec![floor[0], floor[1]]) // Convert [hall_up, hall_down, cab] to [hall_up, hall_down]
            .collect()
    }

    // Get cab requests in the format needed for the hall request assigner
    pub fn get_cab_requests(&self) -> Vec<bool> {
        self.request_matrix
            .iter()
            .map(|floor| floor[2]) // Get only cab requests
            .collect()
    }

    pub fn update_request_matrix(&mut self, request_matrix: Vec<[bool; 3]>) {
        self.request_matrix = request_matrix;
    }

    //  TODO: Check if correct
    // Get next floor to visit based on current requests
    pub fn get_next_command(&self) -> Option<Message> {
        for (floor, requests) in self.request_matrix.iter().enumerate() {
            if requests.iter().any(|&r| r) { // If there is any request for this floor
                return Some(Message::GotoFloor { go_to_floor: floor as u8 });
            }
        }
        None
    }

    // Clear requests for a specific floor
    pub fn complete_request_at_floor(&mut self, reached_floor: u8) -> Vec<LightControl> {
        let floor = reached_floor as usize;
        let current_requests = self.request_matrix[floor];
        
        // Get the light controls before clearing the requests
        let lights = LightControl::vec_turn_off_for_from(
            self.identifier,
            reached_floor,
            &current_requests
        );

        // Clear all requests for this floor
        self.request_matrix[floor] = [false; 3];
        
        lights
    }

}