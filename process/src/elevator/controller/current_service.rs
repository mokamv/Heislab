use common::data_struct::CabinState;
use crate::queue::queue::Queue;
use common::data_struct::CallRequest;
use driver_rust::elevio::elev::MotorDirection;

pub(super) struct CurrentService {
    pub(super) request: Option<CallRequest>,
    pub(super) state: CabinState,
    pub(super) serviceable_request: Vec<CallRequest>
}

impl CurrentService {
    pub fn from(state: CabinState) -> Self {
        Self {
            request: None,
            state,
            serviceable_request: vec![],
        }
    }

    fn is_init(&self) -> bool {
        self.request.is_some()
    }

    fn reset(&mut self) {
        self.request = None
    }

    fn update_request(&mut self, request: CallRequest) {
        self.request = Some(request);
    }

    fn already_serviceable(&self, new_request: &CallRequest) -> bool {
        self.serviceable_request.contains(new_request)
    }

    fn is_current_request(&self, new_request: &CallRequest) -> bool {
        self.request.clone().unwrap().eq(new_request)
    }

    fn update_serviceable(&mut self, queue: &mut Queue) {
        debug_assert!(self.is_init(), "The current service need to be initialized");
        self.serviceable_request.clear();

        let newly_serviceable = queue.retain(|request: &CallRequest|
            ! self.is_current_request(request) &&
                ! self.already_serviceable(request) &&
                ! self.is_serviceable(request));

        for s_req in newly_serviceable {
            if ! self.already_serviceable(&s_req)
                && ! self.is_current_request(&s_req) {
                self.add_to_serviceable(s_req);
            }
        }
    }

    fn is_serviceable(&self, new_request: &CallRequest) -> bool {
        let current_request = self.request.clone().unwrap();

        debug_assert!(! self.already_serviceable(new_request), "An already serviceable request is not serviceable again");
        debug_assert!(! self.is_current_request(new_request), "The current request is not serviceable again");

        if self.is_final_floor(new_request.target()) {
            return false
        }

        let current_direction = self.state.get_direction_to(current_request.target());
        let current_floor = self.state.get_current_floor();

        let is_new_request_direction_ok = match new_request {
            CallRequest::Cab { .. } => true,
            CallRequest::Hall{ direction, .. } => current_direction.eq(direction)
        };

        is_new_request_direction_ok && match current_direction {
            MotorDirection::Up => current_floor < new_request.target()
                && new_request.target() < current_request.target(),
            MotorDirection::Down => current_floor > new_request.target()
                && new_request.target() > current_request.target(),
            MotorDirection::Stop => false,
        }
    }

    fn add_to_serviceable(&mut self, request: CallRequest) {
        debug_assert!(self.is_serviceable(&request), "This request is not serviceable");
        self.serviceable_request.push(request);
    }

    fn does_stop(&self, floor: u8) -> bool {
        let original_request = self.request.clone().unwrap();

        if self.is_final_floor(floor) {
            true
        } else {
            let serviceable_hall_req = CallRequest::Hall {
                floor,
                direction: CabinState::get_direction_from_to(floor, original_request.target())
            };
            let serviceable_call_req = CallRequest::Cab { floor };

            for other_req in self.serviceable_request.iter() {
                if serviceable_call_req.eq(other_req) || serviceable_hall_req.eq(other_req) {
                    return true
                }
            }
            false
        }
    }

    fn is_final_floor(&self, floor: u8) -> bool {
        self.request.clone().unwrap().target() == floor
    }

    fn remove_serviced(&mut self, floor: u8) -> Vec<CallRequest> {
        debug_assert!(!self.is_final_floor(floor), "Cannot removed serviced when the request is the final one");
        debug_assert!(self.does_stop(floor), "Cannot remove serviced request when none were serviced.");

        let original_request = self.request.clone().unwrap();
        let mut serviced = Vec::new();

        let serviceable_hall_req = CallRequest::Hall {
            floor,
            direction: CabinState::get_direction_from_to(floor, original_request.target())
        };
        let serviceable_call_req = CallRequest::Cab { floor };

        let mut i = 0;
        while i < self.serviceable_request.len() {
            let other_req = self.serviceable_request.get(i).unwrap();
            if *other_req == serviceable_call_req || *other_req == serviceable_hall_req {
                serviced.push(self.serviceable_request.swap_remove(i));
            }
            i += 1;
        }
        serviced
    }
}