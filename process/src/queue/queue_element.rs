use std::cell::RefCell;
use std::rc::Rc;
use common::data_struct::CallRequest;

pub struct QueueElement {
    pub(crate) request: CallRequest,
    pub(super) previous: Option<Rc<RefCell<QueueElement>>>, // Pointer to the previous element in the queue
    pub(super) next: Option<Rc<RefCell<QueueElement>>>, // Pointer to the next element in the queue
}

impl QueueElement {
    pub fn new(request: CallRequest) -> QueueElement {
        QueueElement {
            request,
            previous: None,
            next: None
        }
    }
}

impl Default for QueueElement {
    fn default() -> Self {
        QueueElement {
            request: CallRequest::Cab { floor: u8::MAX },
            previous: None,
            next: None,
        }
    }
}