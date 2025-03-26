use std::array;
use std::process::Command;

use crate::elevator::controller::elevator_fsm::ElevatorState;
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
                    CabinState::Idle { .. } => {
                        let next_command = elevator.get_next_command();
                        if let Some(next_command) = next_command {
                            self.client_pool.send(
                                Target::Specific(identifier),
                                next_command
                            ).unwrap()
                        }
                    }
                    CabinState::Between { .. } => {}
                    CabinState::Init => {}
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

    pub fn execute_hall_request_assigner(&self) -> Result<(), String> {
        let hall_requests: Vec<[bool; 3]> = self.pool.iter()
            .filter(|elevator| elevator.is_connected())
            .filter(|elevator| !elevator.get_state().is_init())
            .map(|elevator| elevator.get_hall_requests())
            .collect();

        let states: HashMap<String, serde_json::Value> = self.pool.iter()
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

        // Create input JSON for hall request assigner
        let input_json = json!({
            "hallRequests": hall_requests,
            "states": states,
        }).to_string();
        
        // Execute hall request assigner
        let output = Command::new("hall_request_assigner")
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

        self.assign_updated_elevator_states(hall_requests_assignments);

        Ok(())
    }

    fn assign_updated_elevator_states(&mut self, hall_requests_assignments: HashMap<String, Vec<[bool; 3]>>) {
        for (elevator_id, hall_requests) in hall_requests_assignments {
            let elevator_id = ConnectionIdentifier::from(elevator_id);
            let elevator = self.get_elevator(elevator_id);

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