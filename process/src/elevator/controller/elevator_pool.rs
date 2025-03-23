use std::process::Command;

use crate::elevator::controller::elevator_state::ElevatorState;
use common::connection::client_pool::client_pool::{ClientPool, Target};
use common::connection::connection_handle::handle::ConnectionIdentifier;
use common::data_struct::{CabinState, CallRequest};
use common::messages::Message;
use crate::elevator::controller::light_control::LightControl;
use std::collections::HashMap;
use std::collections::HashMap;
use serde_json::json;


pub struct ElevatorPool {
    pool: Vec<ElevatorState>,
    client_pool: ClientPool,
}

impl ElevatorPool {
    pub fn from(client_pool: ClientPool) -> Self {
        let mut elevators = vec![];
        client_pool.client_identifiers().iter().for_each(|elevator_id| {
            elevators.push(ElevatorState::from(*elevator_id))
        });

        Self {
            pool: elevators,
            client_pool,
        }
    }

    fn get_elevator(&mut self, elevator_id: ConnectionIdentifier) -> &mut ElevatorState {
        self.pool.iter_mut().find(|candidate| candidate.identifier() == elevator_id).unwrap()
    }

    pub fn handle_elevator_message(
        &mut self,
        identifier: ConnectionIdentifier,
        message: Message
    ) {
        let mut elevator = self.get_elevator(identifier);
        match message {
            Message::Connected => {
                elevator.set_connected(false);
                println!("Connected to a client");
            }
            Message::Authenticated => {
                elevator.set_connected(true);
                println!("Identified to a client")
            }

            Message::Disconnected => {
                elevator.set_connected(false);
                println!("Disconnected from client");
            }

            Message::ClientButtonCall { pressed: request } => {
                if let CallRequest::Hall { .. } = request {
                    elevator = self.best_elevator(request);
                }
                elevator.add_and_get_new_target(request);
                let can_receive = elevator.can_receive();
                let new_request = elevator.get_next_command();

                LightControl::turn_on_for_from(identifier, request)
                    .send(&mut self.client_pool);

                // Update state immediately if possible.
                if can_receive {
                    if let Some(new_request) = new_request {
                        self.client_pool.send(
                            Target::Specific(identifier),
                            new_request
                        ).unwrap()
                    }
                }
            }

            // TODO
            Message::ClientObstructed { .. } => {}

            Message::ClientCabinState { cabin_state } => {
                elevator.set_state(cabin_state);
                match cabin_state {
                    CabinState::DoorOpen { current_floor } => {
                        let lights = elevator.complete_request_at_floor(current_floor);
                        for light_control in lights {
                            light_control.send(&mut self.client_pool)
                        }
                    }
                    CabinState::DoorClose { .. } => {
                        let next_command = elevator.get_next_command();
                        if let Some(next_command) = next_command {
                            self.client_pool.send(
                                Target::Specific(identifier),
                                next_command
                            ).unwrap()
                        }
                    }
                    CabinState::Between { .. } => {}
                }
            }


            Message::ClientStopButton { .. } => println!("Unimplemented"),



            Message::ControllerAddress { .. } => {}
            Message::ClientAuth { .. } => {}
            Message::ControllerAuth { .. } => {}
            Message::ControllerCurrentState { .. } => {}

            _ => unreachable!()
        }
    }

    pub fn execute_hall_request_assigner(&self) -> Result<HashMap<String, Vec<Vec<bool>>>, String> {
        let hall_requests: Vec<Vec<bool>> = self.pool.iter()
            .map(|elevator| elevator.get_hall_requests_cost_input())
            .collect();

        let states: HashMap<String, _> = self.pool.iter()
            .map(|elevator| {
                let state = elevator.get_state();
                let id = elevator.identifier().to_string();
                
                let behaviour = match state {
                    CabinState::DoorClose { current_floor } => "idle",
                    CabinState::Between { from_floor, to_floor } => "moving",
                    CabinState::DoorOpen => "doorOpen",
                };

                let floor = state.get_last_seen_floor();

                let direction = match state.get_direction() {
                    MotorDirection::Up => "up",
                    MotorDirection::Down => "down",
                    MotorDirection::Stop => "stop",
                };

                let cab_requests = state.get_cab_requests_cost_input();

                (id, json!({
                    "behaviour": behaviour,
                    "floor": floor,
                    "direction": direction,
                    "cabRequests": cab_requests,
                }))
            })
            .collect();

        let input_json = json!({
            "hallRequests": hall_requests,
            "states": states,
        }).to_string();
        
        let output = Command::new("hall_request_assigner")
            .arg("--input")
            .arg(input_json)
            .output()
            .expect("Failed to run hall request assigner");

        if output.status.success() {
            let output_json = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
            let hall_requests_assignments: HashMap<String, Vec<Vec<bool>>> = serde_json::from_str(&output_json).map_err(|e| e.to_string())?;
            Ok(hall_requests_assignments)
        } else {
            Err(String::from_utf8(output.stderr).map_err(|e| e.to_string())?)
        }
    }
}