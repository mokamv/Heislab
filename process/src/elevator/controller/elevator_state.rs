use common::data_struct::CallRequest;
use common::data_struct::CabinState;
use crate::elevator::controller::current_service::CurrentService;
use crate::queue::queue::Queue;

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
}