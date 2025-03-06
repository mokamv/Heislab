use common::data_struct::CallRequest;
use crate::elevator::controller::current_service::CurrentService;
use crate::queue::queue::Queue;
use crate::elevator::controller::motor_direction::MotorDirection;

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

    pub fn cost(&self, call: CallRequest::Hall) -> i8 {

        let mut cost: i8 = 127; // max cost
        
        let target_floor = call.target();
        let current_floor = self.current_service.state.get_current_floor();
        
        if !self.is_connected {
            return cost; // Max cost is returned if elevator is disconnected
        }

        if !self.current_service.is_init() { // Elevator is idle
            if current_floor == target_floor {
                return -128; // Best case: already at the floor and idle
            } else {
                // Cost is increased according to distance between current floor and target floor when idle
                let distance = (current_floor as i8 - target_floor as i8).abs();
                return distance.min(126); // Cap at 126 to leave room for max cost
            }
        }

        // Elevator is servicing a request
        let current_direction = self.current_service.state.get_direction();

        // Check if call is in the same direction
        let is_same_direction = match current_direction {
            MotorDirection::Stop => true,
            dir => dir == direction
        };

        if is_same_direction {
            let current_target = self.current_service.request.as_ref().unwrap().target();
            
            // Calculating cost based on position of target floor relative to the current path
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
                MotorDirection::Stop => { // if elevator is idle
                    (current_floor as i8 - target_floor as i8).abs()
                }
            };
            cost += (self.main_queue.len() as i8).min(20); // Penalty for number of stops planned
        }

        cost.max(-128).min(127) // To ensure that cost stays within i8 bounds
    }
}