use std::process::{id, Command, Stdio};

use crate::elevator::controller::controller_sync::ControllerSync;
use crate::elevator::controller::elevator_fsm::ElevatorState;
use common::connection::event_handle::controller_handle::controller_handle::{ControllerHandle, Target};
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::data_struct::{CabinState, CallLightArray, CallRequest};
use common::messages::Message;
use driver_rust::elevio::elev::MotorDirection;
use serde_json::json;
use std::collections::HashMap;
use std::env::current_dir;
use std::io::Write;
use std::ops::BitOrAssign;
use std::str::FromStr;
use common::config::N_FLOOR;
use crate::elevator::client::elevator_hardware::ElevatorHardwareState;

pub struct ElevatorPool {
    pool: Vec<ElevatorState>
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

    fn get_elevator_mut(&mut self, elevator_id: ConnectionIdentifier) -> &mut ElevatorState {
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

                let _ = self
                    .execute_hall_request_assigner()
                    .unwrap();

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
                self.handle_obstruction(is_obstructed),

            Message::ClientCabinState { cabin_state } =>
                self.handle_cabin_state(identifier, cabin_state),

            // TODO
            Message::ClientStopButton { .. } => println!("Unimplemented"),

            Message::ClientSyncCab { cab_pressed } => {
                elevator.merge_cab_requests(cab_pressed);
                let new_cab_requests = elevator.get_cab_requests();

                let _ = self
                    .execute_hall_request_assigner()
                    .unwrap();
                let light_array = self.get_merged_call_light_state_for(identifier);
                controller_handle.send_client_message(
                    Target::Specific(identifier),
                    Message::ClientSyncCab {
                        cab_pressed: new_cab_requests
                    }
                );
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
        cabin_state: CabinState
    ) {
        let mut elevator = self.get_elevator_mut(elevator_id);
        elevator.set_state(cabin_state);
        //TODO
        // match cabin_state {
        //     CabinState::DoorOpen { current_floor } => {
        //         let lights = elevator.complete_request_at_floor(current_floor);
        //         for light_control in lights {
        //             light_control.send(&mut self.client_pool)
        //         }
        //     }
        //     CabinState::Idle { .. } => {
        //         let next_command = elevator.get_next_command();
        //         if let Some(next_command) = next_command {
        //             self.client_pool.send(
        //                 Target::Specific(identifier),
        //                 next_command
        //             ).unwrap()
        //         }
        //     }
        //     CabinState::Between { .. } => {}
        //     CabinState::Init => {}
        // }
    }

    fn get_merged_hall_requests(&self) -> [[bool; 2]; N_FLOOR as usize] {
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

    fn get_merged_call_light_state_for(&self, elevator_id: ConnectionIdentifier) -> [[bool; 3]; N_FLOOR as usize] {
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

    fn execute_hall_request_assigner(&mut self) -> Result<(), String> {
        let hall_requests: [[bool; 2]; N_FLOOR as usize] = self.get_merged_hall_requests();

        let states: HashMap<String, serde_json::Value> = self.pool.iter()
            .filter(|elevator| elevator.is_connected() && !elevator.get_state().is_init())
            .map(|elevator| {
                let state = elevator.get_state();
                let id = elevator.identifier().to_string();

                let behaviour = match state {
                    CabinState::Idle { .. } => "idle",
                    CabinState::Between { .. } => "moving",
                    CabinState::DoorOpen { .. } => "doorOpen",
                    _ => unreachable!(),
                };

                let floor = state.get_last_seen_floor();

                let direction = match state.get_direction() {
                    MotorDirection::Up => "up",
                    MotorDirection::Down => "down",
                    MotorDirection::Stop => "stop",
                };

                let cab_requests = elevator.get_cab_requests();

                (id, json!({
                    "behaviour": behaviour,
                    "floor": floor,
                    "direction": direction,
                    "cabRequests": cab_requests,
                }))
            })
            .collect();

        if states.is_empty() {
            return Ok(());
        }

        // Create input JSON for hall request assigner
        let input_json = json!({
            "hallRequests": hall_requests,
            "states": states,
        }).to_string();

        // Execute hall request assigner
        let output = Command::new("./bin/hall_request_assigner")
            .arg("--input")
            .arg(input_json)
            .output()
            .expect("Failed to run hall request assigner");

        if !output.status.success() {
            return Err(String::from_utf8(output.stderr)
                .map_err(|e| e.to_string())?);
        }

        let output_str = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;

        let hall_requests_assignments: HashMap<String, Vec<[bool; 3]>> = serde_json::from_str(&output_str).map_err(|e| e.to_string())?;
        let hall_requests_assignments: HashMap<ConnectionIdentifier, Vec<[bool; 2]>> = hall_requests_assignments
            .iter()
            .map(|(id, requests)| {
                let elevator_id = ConnectionIdentifier::from_str(id).unwrap();
                let requests: Vec<[bool; 2]> = requests.iter()
                    .map(|x| [x[0], x[1]])
                    .collect();


                (elevator_id, requests)
            }).collect();
        
        self.assign_updated_elevator_states(hall_requests_assignments);

        Ok(())
    }

    fn assign_updated_elevator_states(&mut self, hall_requests_assignments: HashMap<ConnectionIdentifier, Vec<[bool; 2]>>) {
        for (elevator_id, hall_requests) in hall_requests_assignments {
            let elevator = self.get_elevator_mut(elevator_id);

            // Clear current hall requests
            elevator.clear_hall_requests();

            // Add new hall requests
            for (floor, requests) in hall_requests.iter().enumerate() {
                if requests[0] { // Up request
                    elevator.add_request(CallRequest::Hall {
                        floor: floor as u8,
                        direction: MotorDirection::Up,
                    });
                }
                if requests[1] { // Down request
                    elevator.add_request(CallRequest::Hall {
                        floor: floor as u8,
                        direction: MotorDirection::Down,
                    });
                }
            }
        }
    }
}