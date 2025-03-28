use crate::elevator::controller::elevator_pool::ElevatorPool;
use common::connection::event_handle::controller_handle::controller_handle::{ControllerHandle, Target};
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::constants::{HALL_DOWN_IDX, HALL_UP_IDX, N_BUTTONS};
use common::data_structures::cabin_state::CabinState;
use common::data_structures::call_request::CallRequest;
use common::data_structures::network::message::Message;
use common::{config::N_FLOOR, constants::N_HALL_BUTTONS};
use driver_rust::elevio::elev::MotorDirection;
use serde_json::json;
use std::collections::HashMap;
use std::process::Command;
use std::str::FromStr;

pub(super) fn execute_hall_request_assigner(
    elevators: &mut ElevatorPool
) -> Result<(), String> {
    let hall_requests: [[bool; N_HALL_BUTTONS]; N_FLOOR as usize] = elevators.get_merged_hall_requests();

    // Create states-input JSON for hall request assigner
    let states: HashMap<String, serde_json::Value> = elevators.pool.iter()
        .filter(|elevator| {
            // Ignore elevators that are not connected, in initialization state or obstructed
            elevator.is_connected()
                && !elevator.get_state().is_init()
                && !elevator.is_obstructed()
        })
        .map(|elevator| {
            let state = elevator.get_state();
            let id = elevator.get_elevator_identifier().to_string();

            // Convert state values to string format
            let behaviour = match state {
                CabinState::Idle { .. } => "idle",
                CabinState::Between { .. } => "moving",
                CabinState::DoorOpen { .. } => "doorOpen",
                _ => unreachable!(),
            };

            let floor = state.get_last_seen_floor();

            // Convert direction to string format
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

    // If there are no elevators to assign hall requests to, return early
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

    // Check if the hall request assigner was successful
    if !output.status.success() {
        return Err(String::from_utf8(output.stderr)
            .map_err(|e| e.to_string())?);
    }

    let output_str = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;

    // Parse output from hall request assigner
    let hall_requests_assignments: HashMap<String, [[bool; N_BUTTONS]; N_FLOOR as usize]> = serde_json::from_str(&output_str).map_err(|e| e.to_string())?;
    let hall_requests_assignments: HashMap<ConnectionIdentifier, [[bool; N_HALL_BUTTONS]; N_FLOOR as usize]> = hall_requests_assignments
        .iter()
        .map(|(id, requests)| {
            let elevator_id = ConnectionIdentifier::from_str(id).unwrap();
            let requests: [[bool; N_HALL_BUTTONS]; N_FLOOR as usize] = requests.iter()
                .map(|x| [x[HALL_UP_IDX], x[HALL_DOWN_IDX]])
                .collect::<Vec<[bool; 2]>>()
                .try_into()
                .unwrap();

            (elevator_id, requests)
        }).collect();

    // Assign new hall requests to elevators
    assign_updated_elevator_states(elevators, hall_requests_assignments);

    Ok(()) // Return success
}

/// Assigns the updated elevator requests from the hall request assigner to the elevators states local request matrix
pub(super) fn assign_updated_elevator_states(
    elevators: &mut ElevatorPool,
    hall_requests_assignments: HashMap<ConnectionIdentifier, [[bool; N_HALL_BUTTONS]; N_FLOOR as usize]>
) {
    // hall_requests_assignments contains the assigned hall requests for each elevator

    // Loop through all elevators in the elevator pool and assign new hall requests
    elevators.pool
        .iter_mut()
        .for_each(|elevator| { // For each elevator
            // Clear current hall requests
            elevator.clear_hall_requests();

            // Get the assigned hall requests for the elevator
            let assigned_requests = hall_requests_assignments
                .get(&elevator.get_elevator_identifier());

            // Loop through all assigned requests and add them to the elevators request matrix
            if let Some(assigned_requests) = assigned_requests {
                for (floor, requests) in assigned_requests.iter().enumerate() {
                    if requests[HALL_UP_IDX] { // Up request
                        elevator.add_request(CallRequest::Hall {
                            floor: floor as u8,
                            direction: MotorDirection::Up,
                        });
                    }
                    if requests[HALL_DOWN_IDX] { // Down request
                        elevator.add_request(CallRequest::Hall {
                            floor: floor as u8,
                            direction: MotorDirection::Down,
                        });
                    }
                }
            }
        });
}

pub(super) fn redistribute_calls(
    elevator_pool: &mut ElevatorPool,
    controller_handle: &ControllerHandle
) {
    for elevator in elevator_pool.pool.iter_mut() {
        let next_floor = elevator.get_next_command();
        if let Some(next_floor) = next_floor {
            controller_handle.send_client_message(
                Target::Specific(elevator.get_elevator_identifier()),
                Message::GotoFloor { go_to_floor: next_floor }
            );
        }
    }
}