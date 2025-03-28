use crate::elevator::controller::controller_sync::ControllerSync;
use crate::elevator::controller::elevator_fsm::ElevatorState;
use crate::elevator::controller::requests_assigner::{execute_hall_request_assigner, redistribute_calls};
use common::config::{CLIENT_COUNT, N_FLOOR};
use common::connection::event_handle::controller_handle::controller_handle::{ControllerHandle, Target};
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::data_structures::cabin_state::CabinState;
use common::data_structures::call_light_array::CallLightArray;
use common::data_structures::call_request::CallRequest;
use common::data_structures::full_requests_matrix::FullControllerRequestsMatrix;
use common::data_structures::network::message::Message;
use std::ops::BitOrAssign;

pub struct ElevatorPool {
    pub(super) pool: [ElevatorState; CLIENT_COUNT as usize]
}

impl ElevatorPool {
    pub fn from(client_identifiers: &[ConnectionIdentifier]) -> Self {
        let elevators = client_identifiers
            .into_iter()
            .map(|elevator_id| {
                ElevatorState::from(*elevator_id)
            })
            .collect::<Vec<ElevatorState>>()
            .try_into()
            .unwrap();

        Self {
            pool: elevators
        }
    }

    fn get_elevator_mut(&mut self, elevator_id: ConnectionIdentifier) -> &mut ElevatorState {
        self.pool.iter_mut().find(|candidate| candidate.get_elevator_identifier() == elevator_id).unwrap()
    }

    fn get_elevator(&self, elevator_id: ConnectionIdentifier) -> &ElevatorState {
        self.pool.iter().find(|candidate| candidate.get_elevator_identifier() == elevator_id).unwrap()
    }

    pub(super) fn handle_elevator_message(
        &mut self,
        controller_sync: &ControllerSync,
        controller_handle: &ControllerHandle,
        elevator_id: ConnectionIdentifier,
        message: Message
    ) {
        let elevator = self.get_elevator_mut(elevator_id);
        match message {
            Message::Connected => self.handle_connect(elevator_id),

            Message::Disconnected => self.handle_disconnect(
                elevator_id,
                controller_handle
            ),

            Message::ClientButtonCall { pressed: request } => {
                // TODO SEND TO CONTROLLER_SYNC FOR SYNC PUPROSE
                elevator.add_request(request);

                let _ = execute_hall_request_assigner(self).unwrap();

                redistribute_calls(self, controller_handle);

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
            }

            Message::ClientObstructed { is_obstructed } =>
                self.handle_obstruction(controller_handle, elevator_id, is_obstructed),

            Message::ClientCabinState { cabin_state } =>
                self.handle_cabin_state(elevator_id, cabin_state, controller_handle),

            // TODO
            Message::ClientStopButton { .. } => println!("Unimplemented"),

            // At connection with a client, the client send it own cab calls
            Message::ClientSyncCab { cab_pressed } => self.handle_client_sync_cab(
                elevator_id,
                cab_pressed,
                controller_handle
            ),

            Message::ClientMotorLocked { is_motor_locked } =>
                self.handle_motor_lock(controller_handle, elevator_id, is_motor_locked),

            _ => unreachable!()
        }
    }

    fn handle_connect(
        &mut self,
        elevator_id: ConnectionIdentifier,
    ) {
        println!("Connected to client");
        let elevator = self.get_elevator_mut(elevator_id);
        elevator.set_connected(true);
    }

    fn handle_disconnect(
        &mut self,
        elevator_id: ConnectionIdentifier,
        controller_handle: &ControllerHandle
    ) {
        println!("Disconnected from client");
        let elevator = self.get_elevator_mut(elevator_id);
        elevator.set_connected(false);

        let _ = execute_hall_request_assigner(self).unwrap();

        redistribute_calls(self, controller_handle);
    }

    fn handle_obstruction(
        &mut self,
        controller_handle: &ControllerHandle,
        elevator_id: ConnectionIdentifier,
        is_obstructed: bool
    ) {
        let elevator = self.get_elevator_mut(elevator_id);
        if elevator.is_obstructed() != is_obstructed {
            elevator.set_obstructed(is_obstructed);

            // Compute requests repartition.
            let _ = execute_hall_request_assigner(self).unwrap();
            // Redistribute calls to every elevator.
            redistribute_calls(self, controller_handle);
        }
    }

    fn handle_motor_lock(
        &mut self,
        controller_handle: &ControllerHandle,
        elevator_id: ConnectionIdentifier,
        is_motor_locked: bool
    ) {
        let elevator = self.get_elevator_mut(elevator_id);
        if elevator.is_motor_locked() != is_motor_locked {
            elevator.set_obstructed(is_motor_locked);

            // Compute requests repartition.
            let _ = execute_hall_request_assigner(self).unwrap();
            // Redistribute calls to every elevator.
            redistribute_calls(self, controller_handle);
        }
    }

    fn handle_client_sync_cab(
        &mut self,
        elevator_id: ConnectionIdentifier,
        cab_pressed: [bool; N_FLOOR as usize],
        controller_handle: &ControllerHandle
    ) {
        let elevator = self.get_elevator_mut(elevator_id);

        // Merge local cab calls with client cab calls
        elevator.merge_cab_requests(cab_pressed);
        let new_cab_requests = elevator.get_cab_requests();

        // Compute requests repartition.
        let _ = execute_hall_request_assigner(self).unwrap();

        // Redistribute calls to every elevator.
        redistribute_calls(self, controller_handle);

        // Send the merged cab calls back to the clients
        controller_handle.send_client_message(
            Target::Specific(elevator_id),
            Message::ClientSyncCab {
                cab_pressed: new_cab_requests
            }
        );

        // Since the client isn't initialized, we send the lights state to the client
        let light_array = self.get_call_lights_state_for(elevator_id);
        controller_handle.send_client_message(
            Target::Specific(elevator_id),
            Message::FullCallLightControl {
                light_array: CallLightArray::from(light_array),
            }
        )
    }

    fn handle_cabin_state(
        &mut self,
        elevator_id: ConnectionIdentifier,
        cabin_state: CabinState,
        controller_handle: &ControllerHandle
    ) {
        let elevator = self.get_elevator_mut(elevator_id);
        elevator.set_state(cabin_state);

        match cabin_state {
            CabinState::Idle { .. } => {
                let next_command = elevator.get_next_command();
                if let Some(next_command) = next_command {
                    controller_handle.send_client_message(
                        Target::Specific(elevator_id),
                        Message::GotoFloor { go_to_floor: next_command }
                    );
                }
            }
            CabinState::DoorOpen { current_floor, .. } => {
                // Clear finished request from the elevator request matrix, and turn off the light
                // If door is open, and there are hall_requests there on their way up or down, we
                // also have to clear the request and turn off the light accordingly

                // Clear all relevant call requests
                elevator.on_door_open_clear_cab_request();
                let cleared_requests =
                    elevator.on_door_open_clear_relevant_hall_requests();

                // Update lights as well.
                for cleared_request in cleared_requests {
                    controller_handle.send_client_message(
                        Target::All,
                        Message::LightControl {
                            button: cleared_request,
                            is_lit: false
                        }
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

    pub(super) fn get_clients_call_requests(&self) -> [[bool; CLIENT_COUNT]; N_FLOOR as usize] {
        let mut clients_cab_requests = [[false; CLIENT_COUNT]; N_FLOOR as usize];

        let raw_cab_requests: [bool; N_FLOOR as usize * CLIENT_COUNT] = self.pool
            .iter()
            .map(|elevator_state| elevator_state.get_cab_requests())
            .flatten()
            .collect::<Vec<bool>>()
            .try_into()
            .unwrap();

        clients_cab_requests.iter_mut()
            .enumerate()
            .for_each(|(floor_idx, floor_array)| {
                floor_array.iter_mut()
                    .enumerate()
                    .for_each(|(client_idx, client_floor_value)| {
                        *client_floor_value = raw_cab_requests[client_idx * N_FLOOR as usize + floor_idx];
                    })
            });
        clients_cab_requests
    }

    pub(super) fn get_full_requests_matrix(&self) -> FullControllerRequestsMatrix {
        let merged_hall_requests = self.get_merged_hall_requests();
        let clients_cab_requests = self.get_clients_call_requests();

        FullControllerRequestsMatrix::from(
            merged_hall_requests,
            clients_cab_requests
        )
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