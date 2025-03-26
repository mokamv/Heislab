// use crate::elevator::controller::elevator_service::ElevatorService;
// use crate::elevator::controller::light_control::LightControl;
// use common::connection::connection_handle::handle::ConnectionIdentifier;
// use common::data_struct::{CabinState, CallRequest};
// use common::messages::TimedMessage;
// use std::collections::VecDeque;
// use driver_rust::elevio::elev::MotorDirection;
// const QUEUE_PENALTY: i32 = 20;
// 
// pub struct ElevatorState {
//     identifier: ConnectionIdentifier,
//     is_connected: bool,
//     state: CabinState,
//     queue: VecDeque<ElevatorService>,
// }
// 
// impl ElevatorState {
//     pub(super) fn from(identifier: ConnectionIdentifier) -> Self {
//         Self {
//             identifier,
//             is_connected: false,
//             state: Default::default(),
//             queue: Default::default(),
//         }
//     }
// 
//     pub(super) fn identifier(&self) -> ConnectionIdentifier {
//         self.identifier
//     }
// 
//     pub fn set_connected(&mut self, is_connected: bool) {
//         self.is_connected = is_connected;
//     }
// 
//     pub fn set_state(&mut self, state: CabinState) {
//         self.state = state;
//     }
// 
//     pub fn can_receive(&self) -> bool {
//         ! self.state.is_door_open()
//     }
// 
//     pub(super) fn add_and_get_new_target(&mut self, request: CallRequest) {
//         // On empty queue (idling elevator)
//         if self.queue.is_empty() {
//             debug_assert!(self.state.is_idle(), "An empty queue must coincide with idling");
//             self.queue.push_front(ElevatorService::from(request, self.state.get_last_seen_floor()));
//         } else {
//             for service in self.queue.iter_mut() {
//                 // Drops already serviced request
//                 if service.is_already_in(&request) {
//                     return;
//                 }
//                 // If possible, upgrade the request to go further on the planned direction.
//                 else if service.is_upgradeable_with(&request) {
//                     service.upgrade_to(request);
//                     return;
//                 }
//                 // Finally check if the request can even fit inside this service
//                 else if service.can_add(&request, &self.state) {
//                     service.add(request);
//                     return;
//                 }
//             };
// 
//             self.queue.push_back(ElevatorService::from(request, self.state.get_last_seen_floor()));
//         }
//     }
// 
//     pub fn get_next_command(&self) -> Option<TimedMessage> {
//         let current_service = self.queue.front()?;
//         let next_floor = current_service.get_next_serviceable_floor();
//         Some(TimedMessage::GotoFloor { go_to_floor: next_floor })
//     }
// 
//     pub fn complete_request_at_floor(&mut self, reached_floor: u8) -> Vec<LightControl> {
//         // Door should only open when there is a request associated
//         if let Some(current_service) = self.queue.front_mut() {
//             if current_service.is_final_floor(reached_floor) {
//                 let current_service = self.queue.pop_front().unwrap();
//                 let serviced_calls = current_service.last_floor_serviced();
//                 LightControl::vec_turn_off_for_from(self.identifier, serviced_calls)
//             }
// 
//             else {
//                 let serviced_calls = current_service.remove_serviced(reached_floor);
//                 LightControl::vec_turn_off_for_from(self.identifier, serviced_calls)
//             }
//         }
//         else {
//             panic!("Cannot complete a nonexistent request");
//         }
//     }
// 
//     pub fn cost(&self, call: CallRequest) -> i8 {
//         if !self.is_connected {
//             return i8::MAX; // Return max cost if elevator is disconnected
//         }
// 
//         let target_floor = call.target();
//         let current_floor = self.state.get_last_seen_floor();
// 
//         // If idle, cost is purely distance-based
//         if !self.queue.is_empty() {
//             return if current_floor == target_floor {
//                 i8::MIN // Already at the target floor
//             } else {
//                 (current_floor as i8 - target_floor as i8).abs().min(i8::MAX - 1) // Cap at 126 to leave room for max cost
//             };
//         }
// 
//         let current_service = self.queue.front().unwrap();
// 
//         // Elevator is already servicing a request
//         let current_direction = current_service.direction();
//         let call_direction = call.direction();
// 
//         // Check if call is in the same direction
//         let is_same_direction = match call_direction {
//             None => true,
//             Some(call_direction) => {
//                 match current_direction {
//                     MotorDirection::Stop => true,
//                     dir => dir == call_direction
//                 }
//             }
//         };
// 
// 
//         let mut cost = i8::MAX;
// 
//         if is_same_direction {
//             let current_target = current_service.final_floor();
// 
//             // Calculating cost based on position of target floor relative to the current path
//             cost = match current_direction {
//                 MotorDirection::Up => {
//                     if target_floor >= current_floor && target_floor <= current_target {
//                         // On the way up
//                         (target_floor as i8 - current_floor as i8).abs()
//                     } else {
//                         // Will need to come back
//                         ((current_target as i8 - current_floor as i8).abs() +
//                             (current_target as i8 - target_floor as i8).abs())
//                     }
//                 },
//                 MotorDirection::Down => {
//                     if target_floor <= current_floor && target_floor >= current_target {
//                         // On the way down
//                         (current_floor as i8 - target_floor as i8).abs()
//                     } else {
//                         // Will need to come back
//                         ((current_floor as i8 - current_target as i8).abs() +
//                             (target_floor as i8 - current_target as i8).abs())
//                     }
//                 },
//                 MotorDirection::Stop => { // if elevator is idle
//                     (current_floor as i8 - target_floor as i8).abs()
//                 }
//             };
// 
// 
// 
//             let mut potential_penalty = 0;
//             for service in self.queue.iter() {
//                 potential_penalty += service.number_of_stops();
//             }
// 
//             cost += (potential_penalty as i32).min(QUEUE_PENALTY) as i8; // Penalty for number of stops planned
//         }
// 
//         cost
//     }
// }