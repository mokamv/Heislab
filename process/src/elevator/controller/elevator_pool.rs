use crate::elevator::controller::controller_sync::ControllerSync;
use crate::elevator::controller::elevator_fsm::ElevatorState;
use crate::elevator::controller::requests_assigner::execute_hall_request_assigner;
use common::config::N_FLOOR;
use common::connection::event_handle::controller_handle::controller_handle::{ControllerHandle, Target};
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::data_struct::{CabinState, CallLightArray, CallRequest};
use common::messages::Message;
use std::ops::BitOrAssign;

pub struct ElevatorPool {
    pub(super) pool: Vec<ElevatorState>
}

impl ElevatorPool {
    pub fn from(client_identifiers: &[ConnectionIdentifier]) -> Self {
        let mut elevators = vec![];
        client_identifiers.into_iter().for_each(|elevator_id| {
            elevators.push(ElevatorState::from(*elevator_id))
        });

        Self {
            pool: elevators
        }
    }

    pub(super) fn get_elevator_mut(&mut self, elevator_id: ConnectionIdentifier) -> &mut ElevatorState {
        self.pool.iter_mut().find(|candidate| candidate.identifier() == elevator_id).unwrap()
    }

    fn get_elevator(&self, elevator_id: ConnectionIdentifier) -> &ElevatorState {
        self.pool.iter().find(|candidate| candidate.identifier() == elevator_id).unwrap()
    }

    pub fn handle_elevator_message(
        &mut self,
        controller_sync: &ControllerSync,
        controller_handle: &ControllerHandle,
        identifier: ConnectionIdentifier,
        message: Message
    ) {
        let elevator = self.get_elevator_mut(identifier);
        match message {
            Message::Connected => {
                elevator.set_connected(true);
                println!("Connected to a client");
            }

            Message::Disconnected => {
                elevator.set_connected(false);
                println!("Disconnected from client");
            }

            Message::ClientButtonCall { pressed: request } => {
                // TODO SEND TO CONTROLLER_SYNC FOR SYNC PUPROSE
                elevator.add_request(request);

                let _ = execute_hall_request_assigner(self).unwrap();

                // Send a message to synchronize lights on every client
                if let CallRequest::Hall { .. } = request {
                    controller_handle.send_client_message(
                        Target::All,
                        Message::LightControl {
                            button: request,
                            is_lit: true
                        }
                    )
                };

                for elevator in self.pool.iter() {
                    elevator.fsm_on_request_button_press(controller_handle);
                }
            }

            Message::ClientObstructed { is_obstructed } =>
                self.handle_obstruction(is_obstructed),

            Message::ClientCabinState { cabin_state } =>
                self.handle_cabin_state(identifier, cabin_state, controller_handle),

            // TODO
            Message::ClientStopButton { .. } => println!("Unimplemented"),

            // At connection with a client, the client send it own cab calls
            Message::ClientSyncCab { cab_pressed } => {
                // Merge local cab calls with client cab calls
                elevator.merge_cab_requests(cab_pressed);
                let new_cab_requests = elevator.get_cab_requests();

                // Compute requests repartition
                let _ = execute_hall_request_assigner(self).unwrap();

                // Send the merged cab calls back to the clients
                controller_handle.send_client_message(
                    Target::Specific(identifier),
                    Message::ClientSyncCab {
                        cab_pressed: new_cab_requests
                    }
                );

                // Since the client isn't initialized, we send the lights state to the client
                let light_array = self.get_call_lights_state_for(identifier);
                controller_handle.send_client_message(
                    Target::Specific(identifier),
                    Message::FullCallLightControl {
                        light_array: CallLightArray::from(light_array),
                    }
                )
            }

            _ => unreachable!()
        }
    }

    fn handle_obstruction(&mut self, is_obstructed: bool) {
        // TODO
    }

    fn handle_cabin_state(
        &mut self,
        elevator_id: ConnectionIdentifier,
        cabin_state: CabinState,
        controller_handle: &ControllerHandle
    ) {
        let mut elevator = self.get_elevator_mut(elevator_id);
        elevator.set_state(cabin_state);

        match cabin_state {
            CabinState::Idle { .. } => {
                let next_command = elevator.get_next_command();
                if let Some(next_command) = next_command {
                    controller_handle.send_client_message(
                        Target::Specific(self.identifier),
                        Message::GotoFloor { go_to_floor: next_command.unwrap() }
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn get_merged_hall_requests(&self) -> [[bool; 2]; N_FLOOR as usize] {
        self.pool.iter()
            .filter(|elevator| !elevator.get_state().is_init())
            .map(|elevator| elevator.get_hall_requests())
            .fold([[false; 2]; N_FLOOR as usize], |mut acc, b| {
                for i in 0..acc.len() {
                    for j in 0..2 {
                        acc.get_mut(i).unwrap()[j].bitor_assign(b.get(i).unwrap()[j]);
                    }
                }
                acc
            })
    }

    fn get_call_lights_state_for(&self, elevator_id: ConnectionIdentifier) -> [[bool; 3]; N_FLOOR as usize] {
        let elevator = self.get_elevator(elevator_id);
        let cab_requests = elevator.get_cab_requests();
        let hall_requests = self.get_merged_hall_requests();

        hall_requests.into_iter()
            .zip(cab_requests.into_iter())
            .map(|(hall_reqs, cab_req)| {
                [hall_reqs[0], hall_reqs[1], cab_req]
            })
            .collect::<Vec<[bool; 3]>>()
            .try_into()
            .unwrap()
    }
}