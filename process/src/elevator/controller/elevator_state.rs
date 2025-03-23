use crate::elevator::controller::elevator_service::ElevatorService;
use crate::elevator::controller::light_control::LightControl;
use common::connection::connection_handle::handle::ConnectionIdentifier;
use common::data_struct::{CabinState, CallRequest};
use common::messages::Message;
use std::collections::VecDeque;
use driver_rust::elevio::elev::MotorDirection;

pub struct ElevatorState {
    identifier: ConnectionIdentifier,
    is_connected: bool,
    state: CabinState,
    queue: VecDeque<ElevatorService>,
}

impl ElevatorState {
    pub(super) fn from(identifier: ConnectionIdentifier) -> Self {
        Self {
            identifier,
            is_connected: false,
            state: Default::default(),
            queue: Default::default(),
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

    pub fn get_state_mut(&self) -> &CabinState {
        &self.state
    }

    pub fn get_queue(&self) -> &VecDeque<ElevatorService> {
        &self.queue
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub fn can_receive(&self) -> bool {
        ! self.state.is_door_open()
    }

    pub(super) fn add_and_get_new_target(&mut self, request: CallRequest) {
        // On empty queue (idling elevator)
        if self.queue.is_empty() {
            debug_assert!(self.state.is_idle(), "An empty queue must coincide with idling");
            self.queue.push_front(ElevatorService::from(request, self.state.get_last_seen_floor()));
        } else {
            for service in self.queue.iter_mut() {
                // Drops already serviced request
                if service.is_already_in(&request) {
                    return;
                }
                // If possible, upgrade the request to go further on the planned direction.
                else if service.is_upgradeable_with(&request) {
                    service.upgrade_to(request);
                    return;
                }
                // Finally check if the request can even fit inside this service
                else if service.can_add(&request, &self.state) {
                    service.add(request);
                    return;
                }
            };

            self.queue.push_back(ElevatorService::from(request, self.state.get_last_seen_floor()));
        }
    }

    pub fn get_next_command(&self) -> Option<Message> {
        let current_service = self.queue.front()?;
        let next_floor = current_service.get_next_serviceable_floor();
        Some(Message::GotoFloor { go_to_floor: next_floor })
    }

    pub fn complete_request_at_floor(&mut self, reached_floor: u8) -> Vec<LightControl> {
        // Door should only open when there is a request associated
        if let Some(current_service) = self.queue.front_mut() {
            if current_service.is_final_floor(reached_floor) {
                let current_service = self.queue.pop_front().unwrap();
                let serviced_calls = current_service.last_floor_serviced();
                LightControl::vec_turn_off_for_from(self.identifier, serviced_calls)
            }

            else {
                let serviced_calls = current_service.remove_serviced(reached_floor);
                LightControl::vec_turn_off_for_from(self.identifier, serviced_calls)
            }
        }
        else {
            panic!("Cannot complete a nonexistent request");
        }
    }

    // The following functions are used for cost function algorithm in elevator_pool.rs.
    // The cost function algorithm is used to assign hall requests to elevators.

    pub fn get_total_floors(&self) -> usize { // FIX, do not hardcode
        4
    }

    pub fn get_hall_requests_cost_input(&self) -> Vec<Vec<bool>> {
        let total_floors = self.get_total_floors();
        let mut hall_requests = vec![vec![false, false]; total_floors];

        for request in &self.queue {
            if let CallRequest::Hall { floor, direction } = request {
                match direction {
                    MotorDirection::Up => hall_requests[*floor as usize][0] = true,
                    MotorDirection::Down => hall_requests[*floor as usize][1] = true,
                    MotorDirection::Stop => {}
                }
            }
        }
        hall_requests
    }

    pub fn get_cab_requests_cost_input(&self) -> Vec<bool> {
        let total_floors = self.get_total_floors();
        let mut cab_requests = vec![false; total_floors];

        for request in &self.queue {
            if let CallRequest::Cab { floor } = request {
                cab_requests[*floor as usize] = true;
            }
        }

        cab_requests
    }

}