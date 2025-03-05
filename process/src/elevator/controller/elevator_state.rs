use common::data_struct::CallRequest;
use common::data_struct::CabinState;
use crate::elevator::controller::current_service::CurrentService;
use crate::queue::queue::Queue;
use std::cmp::Ordering;

pub struct ElevatorState {
    is_connected: bool,
    main_queue: Queue,
    current_service: CurrentService,
}

impl Default for ElevatorState {
    fn default() -> Self {
        Self {
            is_connected: false,
            main_queue: Queue::new(8),
            current_service: CurrentService::from(Default::default())
        }
    }
}

impl ElevatorState {
    pub fn set_connected(&mut self, is_connected: bool) {
        self.is_connected = is_connected;
    }

    pub fn handle_button_call(&self, button: CallRequest) {

    }

    pub fn handle_floor_reached(&self, floor_reached: u8) {

    }

    pub fn handle_obstruction(&self, ) {

    }

    pub fn cost(&self, call: CallRequest) -> i8 {
        // Start with maximum cost
        let mut cost: i8 = 127;
        
        let target_floor = call.target();
        let current_floor = self.current_service.state.get_current_floor();
        
        if !self.is_connected {
            return cost; // Return max cost if elevator is disconnected
        }

        if !self.current_service.is_init() {
            // Elevator is idle
            match current_floor.cmp(&target_floor) {
                Ordering::Equal => return -128, // Best case: already at the floor and idle
                _ => {
                    // Cost increases with distance when idle
                    let distance = (current_floor as i8 - target_floor as i8).abs();
                    return distance.min(126); // Cap at 126 to leave room for max cost
                }
            }
        }

        // Elevator is servicing a request
        let current_direction = self.current_service.state.get_direction();
        let call_direction = match call {
            CallRequest::Hall { direction, .. } => Some(direction),
            CallRequest::Cab { .. } => None
        };

        // Check if call is in the same direction
        let is_same_direction = match (current_direction, call_direction) {
            (MotorDirection::Stop, _) => true,
            (_, None) => true, // Cab calls can be serviced in any direction
            (dir1, Some(dir2)) => dir1 == dir2
        };

        if is_same_direction {
            let current_target = self.current_service.request.as_ref().unwrap().target();
            
            // Calculate base cost based on position relative to current path
            cost = match current_direction {
                MotorDirection::Up => {
                    if target_floor >= current_floor && target_floor <= current_target {
                        // On the way up
                        (target_floor as i8 - current_floor as i8).abs()
                    } else {
                        // Will need to come back
                        ((current_target as i8 - current_floor as i8).abs() + 
                         (current_target as i8 - target_floor as i8).abs())
                    }
                },
                MotorDirection::Down => {
                    if target_floor <= current_floor && target_floor >= current_target {
                        // On the way down
                        (current_floor as i8 - target_floor as i8).abs()
                    } else {
                        // Will need to come back
                        ((current_floor as i8 - current_target as i8).abs() + 
                         (target_floor as i8 - current_target as i8).abs())
                    }
                },
                MotorDirection::Stop => {
                    (current_floor as i8 - target_floor as i8).abs()
                }
            };

            // Add penalty for number of stops already planned
            cost += (self.main_queue.len() as i8).min(20);
        }

        // Ensure cost stays within i8 bounds
        cost.max(-128).min(127)
    }
}