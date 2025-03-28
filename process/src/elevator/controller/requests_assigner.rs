use std::collections::HashMap;
use std::process::Command;
use std::str::FromStr;
use driver_rust::elevio::elev::MotorDirection;
use serde_json::json;
use common::config::N_FLOOR;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::data_structures::cabin_state::CabinState;
use common::data_structures::call_request::CallRequest;
use crate::elevator::controller::elevator_pool::ElevatorPool;

pub(super) fn execute_hall_request_assigner(
    elevators: &mut ElevatorPool
) -> Result<(), String> {
    let hall_requests: [[bool; 2]; N_FLOOR as usize] = elevators.get_merged_hall_requests();

    let states: HashMap<String, serde_json::Value> = elevators.pool.iter()
        .filter(|elevator| {
            elevator.is_connected()
                && !elevator.get_state().is_init()
                && !elevator.is_obstructed()
        })
        .map(|elevator| {
            let state = elevator.get_state();
            let id = elevator.get_elevator_identifier().to_string();

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

    println!("{input_json}");


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

    let hall_requests_assignments: HashMap<String, [[bool; 3]; N_FLOOR as usize]> = serde_json::from_str(&output_str).map_err(|e| e.to_string())?;
    let hall_requests_assignments: HashMap<ConnectionIdentifier, [[bool; 2]; N_FLOOR as usize]> = hall_requests_assignments
        .iter()
        .map(|(id, requests)| {
            let elevator_id = ConnectionIdentifier::from_str(id).unwrap();
            let requests: [[bool; 2]; N_FLOOR as usize] = requests.iter()
                .map(|x| [x[0], x[1]])
                .collect::<Vec<[bool; 2]>>()
                .try_into()
                .unwrap();

            (elevator_id, requests)
        }).collect();

    assign_updated_elevator_states(elevators, hall_requests_assignments);

    Ok(())
}

pub(super) fn assign_updated_elevator_states(
    elevators: &mut ElevatorPool,
    hall_requests_assignments: HashMap<ConnectionIdentifier, [[bool; 2]; N_FLOOR as usize]>
) {
    elevators.pool
        .iter_mut()
        .for_each(|elevator| {
            // Clear current hall requests
            elevator.clear_hall_requests();

            let assigned_requests = hall_requests_assignments
                .get(&elevator.get_elevator_identifier());

            if let Some(assigned_requests) = assigned_requests {
                // Add new hall requests
                for (floor, requests) in assigned_requests.iter().enumerate() {
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
        });
}