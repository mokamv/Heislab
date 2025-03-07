use crate::elevator::controller::elevator_service::ElevatorService;
use crate::elevator::controller::light_control::LightControl;
use common::connection::connection_handle::handle::ConnectionIdentifier;
use common::data_struct::CabinState;
use common::data_struct::CallRequest;
use common::messages::Message;
use std::collections::VecDeque;

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

    pub fn can_receive(&self) -> bool {
        ! self.state.is_door_open()
    }

    pub fn cost(&self, request: CallRequest) -> i8 {
        0
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
}