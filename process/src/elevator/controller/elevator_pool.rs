// use crate::elevator::controller::elevator_state::ElevatorState;
// use common::connection::client_pool::client_pool::{ClientPool, Target};
// use common::connection::connection_handle::handle::ConnectionIdentifier;
// use common::data_struct::{CabinState, CallRequest};
// use common::messages::TimedMessage;
// use crate::elevator::controller::light_control::LightControl;
// 
// pub struct ElevatorPool {
//     pool: Vec<ElevatorState>,
//     client_pool: ClientPool,
// }
// 
// impl ElevatorPool {
//     pub fn from(client_pool: ClientPool) -> Self {
//         let mut elevators = vec![];
//         client_pool.client_identifiers().iter().for_each(|elevator_id| {
//             elevators.push(ElevatorState::from(*elevator_id))
//         });
// 
//         Self {
//             pool: elevators,
//             client_pool,
//         }
//     }
// 
//     fn get_elevator(&mut self, elevator_id: ConnectionIdentifier) -> &mut ElevatorState {
//         self.pool.iter_mut().find(|candidate| candidate.identifier() == elevator_id).unwrap()
//     }
// 
//     pub fn handle_elevator_message(
//         &mut self,
//         identifier: ConnectionIdentifier,
//         message: TimedMessage
//     ) {
//         let mut elevator = self.get_elevator(identifier);
//         match message {
//             TimedMessage::Connected => {
//                 elevator.set_connected(false);
//                 println!("Connected to a client");
//             }
//             TimedMessage::Authenticated => {
//                 elevator.set_connected(true);
//                 println!("Identified to a client")
//             }
// 
//             TimedMessage::Disconnected => {
//                 elevator.set_connected(false);
//                 println!("Disconnected from client");
//             }
// 
//             TimedMessage::ClientButtonCall { pressed: request } => {
//                 if let CallRequest::Hall { .. } = request {
//                     elevator = self.best_elevator(request);
//                 }
//                 elevator.add_and_get_new_target(request);
//                 let can_receive = elevator.can_receive();
//                 let new_request = elevator.get_next_command();
// 
//                 LightControl::turn_on_for_from(identifier, request)
//                     .send(&mut self.client_pool);
// 
//                 // Update state immediately if possible.
//                 if can_receive {
//                     if let Some(new_request) = new_request {
//                         self.client_pool.send(
//                             Target::Specific(identifier),
//                             new_request
//                         ).unwrap()
//                     }
//                 }
//             }
// 
//             // TODO
//             TimedMessage::ClientObstructed { .. } => {}
// 
//             TimedMessage::ClientCabinState { cabin_state } => {
//                 elevator.set_state(cabin_state);
//                 match cabin_state {
//                     CabinState::DoorOpen { current_floor } => {
//                         let lights = elevator.complete_request_at_floor(current_floor);
//                         for light_control in lights {
//                             light_control.send(&mut self.client_pool)
//                         }
//                     }
//                     CabinState::DoorClose { .. } => {
//                         let next_command = elevator.get_next_command();
//                         if let Some(next_command) = next_command {
//                             self.client_pool.send(
//                                 Target::Specific(identifier),
//                                 next_command
//                             ).unwrap()
//                         }
//                     }
//                     CabinState::Between { .. } => {}
//                 }
//             }
// 
// 
//             TimedMessage::ClientStopButton { .. } => println!("Unimplemented"),
// 
// 
// 
//             TimedMessage::ControllerAddress { .. } => {}
//             TimedMessage::ClientAuth { .. } => {}
//             TimedMessage::ControllerAuth { .. } => {}
//             TimedMessage::ControllerCurrentState { .. } => {}
// 
//             _ => unreachable!()
//         }
//     }
// 
//     fn best_elevator(&mut self, request: CallRequest) -> &mut ElevatorState {
//         #[cfg(debug_assertions)]
//         if let CallRequest::Cab {..} = request { panic!("Cannot call this function with a cab call") }
// 
// 
//         self.pool
//             .iter_mut()
//             .min_by(|value_a, value_b| {
//                 value_a.cost(request).cmp(&value_b.cost(request))
//             })
//             .unwrap()
//     }
// }