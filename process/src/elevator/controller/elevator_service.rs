use common::data_struct::CabinState;
use common::data_struct::CallRequest;
use driver_rust::elevio::elev::MotorDirection;

pub(super) struct ElevatorService {
    direction: MotorDirection,
    final_request: CallRequest,
    serviceable_requests: Vec<CallRequest>
}

impl ElevatorService {
    pub(super) fn from(request: CallRequest, departing_floor: u8) -> Self {
        match request {
            CallRequest::Hall { floor, direction } => {
                Self {
                    direction: if departing_floor == floor {
                        direction
                    } else {
                        CabinState::get_direction_from_to(departing_floor, request.target())
                    },
                    final_request: request,
                    serviceable_requests: vec![],
                }
            }
            CallRequest::Cab { .. } => Self {
                direction: CabinState::get_direction_from_to(departing_floor, request.target()),
                final_request: request,
                serviceable_requests: vec![],
            }
        }
    }

    pub(super) fn is_already_in(&self, new_request: &CallRequest) -> bool {
        self.final_request.eq(new_request) || self.serviceable_requests.contains(new_request)
    }

    pub(super) fn is_final_floor(&self, floor: u8) -> bool {
        self.final_request.target() == floor
    }

    pub(super) fn is_upgradeable_with(&self, request: &CallRequest) -> bool {
        match self.final_request {
            CallRequest::Cab { .. } => false,
            CallRequest::Hall { direction, .. } => match direction {
                MotorDirection::Stop => unreachable!(),
                moving => if self.direction == moving {
                    let is_possible_based_on_floor = match moving {
                        MotorDirection::Stop => unreachable!(),
                        MotorDirection::Down => request.target() < self.final_request.target(),
                        MotorDirection::Up => request.target() > self.final_request.target()
                    };

                    is_possible_based_on_floor && if let CallRequest::Hall { direction, .. } = request {
                        *direction == self.direction
                    } else { true }
                } else { false }
            }
        }
    }

    pub(super) fn upgrade_to(&mut self, request: CallRequest) {
        debug_assert!(self.is_upgradeable_with(&request));
        self.serviceable_requests.push(self.final_request);
        self.final_request = request;
    }

    // TODO THERE MIGHT BE A PROBLEM WHEN THE ELEVATOR IS ABOUT TO REACH A FLOOR
    //  IT SHOULDN'T STOP BUT SOMEONE CALLED A STOP THERE THAT IS GOING TO BE SERVICE BY THIS VERY ELEVATOR
    pub(super) fn can_add(&self, new_request: &CallRequest, state: &CabinState) -> bool {
        debug_assert!(! self.is_already_in(new_request), "An already serviceable request is not serviceable again");
        debug_assert!(! self.is_upgradeable_with(new_request), "Upgrade must be preferred over adding to the vec since it will break it");

        // TODO CHANGE THIS, DEPENDING ON THE CONDITION
        if self.is_final_floor(new_request.target()) {
            return false
        }

        // Refuse hall request in opposite direction
        if let CallRequest::Hall { direction, .. } = new_request {
            if self.direction.ne(direction) {
                return false
            }
        }

        let last_seen_floor = state.get_last_seen_floor();

        match self.direction {
            MotorDirection::Up => last_seen_floor < new_request.target()
                && new_request.target() < self.final_request.target(),
            MotorDirection::Down => last_seen_floor > new_request.target()
                && new_request.target() > self.final_request.target(),
            MotorDirection::Stop => false // Always false, current floor service are always single request.
        }
    }

    pub(super) fn add(&mut self, request: CallRequest) {
        self.serviceable_requests.push(request);
    }


    pub(super) fn get_next_serviceable_floor(&self) -> u8 {
        if self.serviceable_requests.is_empty() {
            self.final_request.target()
        } else {
            let serviceable_requests_iter = self.serviceable_requests
                .iter();

            let potential_next_request = match self.direction {
                MotorDirection::Down => serviceable_requests_iter.max_by(|value_a, value_b| {
                    value_a.target().cmp(&value_b.target())
                }),
                MotorDirection::Up => serviceable_requests_iter.min_by(|value_a, value_b| {
                    value_a.target().cmp(&value_b.target())
                }),
                MotorDirection::Stop => return self.final_request.target() // Same floor request are always single request (cab only)
            };

            potential_next_request
                .map(|request| request.target())
                .unwrap()
        }
    }

    pub(super) fn last_floor_serviced(mut self) -> Vec<CallRequest> {
        debug_assert!(self.is_final_floor(self.final_request.target()), "Only callable on last floor");
        for serviceable in self.serviceable_requests.iter() {
            debug_assert_eq!(serviceable.target(), self.final_request.target(), "Some serviceable floor were not serviced before last floor");
        };

        self.serviceable_requests.push(self.final_request);
        self.serviceable_requests
    }

    // TODO "Change floor detection since a counter directed in queue is impossible"
    #[deprecated()]
    pub(super) fn remove_serviced(&mut self, floor: u8) -> Vec<CallRequest> {
        debug_assert!(!self.is_final_floor(floor), "Cannot removed serviced when the request is the final one");

        let mut serviced = Vec::new();
        
        let mut i = 0;
        while i < self.serviceable_requests.len() {
            let other_req = self.serviceable_requests.get(i).unwrap();
            if other_req.target() == floor {
                serviced.push(self.serviceable_requests.swap_remove(i));
            }
            i += 1;
        }
        serviced
    }
}