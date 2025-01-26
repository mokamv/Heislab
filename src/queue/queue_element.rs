use std::cell::RefCell;
use std::rc::Rc;

pub struct QueueElement {
    pub(crate) request: Request,
    pub(super) previous: Option<Rc<RefCell<QueueElement>>>, // Pointer to the previous element in the queue
    pub(super) next: Option<Rc<RefCell<QueueElement>>>, // Pointer to the next element in the queue
}

impl QueueElement {
    pub(super) fn default_req() -> QueueElement {
        QueueElement {
            request: Request::Cab(u8::MAX),
            previous: None,
            next: None,
        }
    }

    pub fn new(request: Request) -> QueueElement {
        QueueElement {
            request,
            previous: None,
            next: None
        }
    }
}

#[derive(Clone, PartialEq)]
#[derive(Debug)]

/// Model a Call from a person pushing one of the button on the cab or hall panel.
pub enum Request {
    Cab(u8),
    Hall(u8, u8)
}

impl Request {
    /// This method returns the 'target' of the request.
    /// 
    /// A `target` is the floor the cabin needs to reach to complete the call
    /// 
    /// A Hall button `target` is the floor the button sits at.
    ///  
    /// A Cab button `target` is the value associated to the button.  
    pub fn target(&self) -> u8 {
        match self {
            Request::Cab(new_target) | Request::Hall(new_target, _) => *new_target
        }
    }

    /// This method return the `direction` of the request, if it has one.
    /// 
    /// Only [Hall](Request::Hall) requests have a `direction`.
    pub fn direction(&self) -> Option<u8> {
        match *self {
            Request::Cab(_) => None,
            Request::Hall(_, direction) => Some(direction)
        }
    }

    /// This method returns the `light_id` associated with the request.
    /// 
    /// A `light_id` is used to turn of and on specific call light on the elevator control panel.
    pub fn light_id(&self) -> u8 {
        match *self {
            Request::Cab(_) => driver_rust::elevio::elev::CAB,
            Request::Hall(_, direction) =>
                match direction {
                    driver_rust::elevio::elev::DIRN_UP => driver_rust::elevio::elev::HALL_UP,
                    driver_rust::elevio::elev::DIRN_DOWN => driver_rust::elevio::elev::HALL_DOWN,
                    _ => panic!("Unexpected State")
                }
        }
    }
}