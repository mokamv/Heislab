use crate::queue::queue_element::{QueueElement, Request};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Queue {
    head: Rc<RefCell<QueueElement>>,
    tail: Rc<RefCell<QueueElement>>,
    size: usize,
    limit: usize
}

impl Queue {
    pub(crate) fn new(limit: usize) -> Queue {
        Queue {
            head: Rc::new(RefCell::new(QueueElement::default_req())),
            tail: Rc::new(RefCell::new(QueueElement::default_req())),
            size: 0,
            limit
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.size == 0
    }
    // fn size(&self) -> usize {
    //     self.size
    // }
    pub(crate) fn pop(&mut self) -> Option<Request> {
        match self.size {
            0 => None,
            1 => {
                let value = self.head.clone();
                self.clear();
                Some(Rc::into_inner(value).unwrap().into_inner().request)
            },
            _ => {
                let current = self.head.clone();
                let next = current.borrow().next.clone().unwrap();
                next.borrow_mut().previous = None;
                self.head = next.clone();
                
                self.size -= 1;
                if self.size == 1 {
                    self.tail = next;
                }
                
                Some(Rc::into_inner(current).unwrap().into_inner().request)
            },
        }
    }

    pub(crate) fn peek(&self) -> Option<Request> {
        if self.is_empty(){
            None
        } else {
            Some(self.head.borrow().request.clone())
        }
    }

    pub(crate) fn push_unique(&mut self, request: Request) -> bool {
        if self.size == self.limit {
            return false;
        }

        //TODO ADD CHECK FOR UNICITY

        let element_ref =  Rc::new(RefCell::new(QueueElement::new(request)));

        if self.is_empty() {
            self.head = element_ref.clone();
            self.tail = element_ref.clone();
        } else {
            {
                let mut tail = self.tail.borrow_mut();
                tail.next = Some(element_ref.clone());
            }
            {
                let mut element = element_ref.borrow_mut();
                element.previous = Some(self.tail.clone());
            }
            self.tail = element_ref;
        }
        self.size += 1;
        true
    }

    pub(crate) fn retain(&mut self, mut condition: impl FnMut(&Request) -> bool) -> Vec<Request> {
        let mut removed = Vec::new();

        let mut pointer = if self.is_empty() {
            None
        } else {
            Some(self.head.clone())
        };

        while let Some(element) = pointer.clone() {
            if ! condition(&element.borrow().request) {
                // Get a reference counter on the (potential) previous and next elements
                let previous_elem = element.borrow().previous.clone();
                let next_elem = element.borrow().next.clone();

                // Clear the element from the reference it still hold.
                element.borrow_mut().previous = None;
                element.borrow_mut().next = None;

                match previous_elem {
                    // This element is the head
                    None => {
                        match next_elem {
                            // This situation means that the list is now empty
                            None => {
                                pointer = None;
                                self.head = Rc::new(RefCell::new(QueueElement::default_req()));
                                self.tail = Rc::new(RefCell::new(QueueElement::default_req()));
                            }
                            // This situation means that the next element become the head
                            Some(next_elem) => {
                                next_elem.borrow_mut().previous = None;
                                pointer = Some(next_elem.clone());
                                self.head = next_elem
                            }
                        }
                    }
                    // This element is not the head
                    Some(previous_elem) => {
                        match next_elem {
                            // This means that the list now have one item (previous_elem)
                            None => {
                                // previous_elem.previous is already set to None since it is the head.
                                previous_elem.borrow_mut().next = None;
                                pointer = None;
                                self.tail = self.head.clone();
                            }
                            // This means that the element have a previous and a next
                            Some(next_elem) => {
                                previous_elem.borrow_mut().next = Some(next_elem.clone());
                                next_elem.borrow_mut().previous = Some(previous_elem);
                                pointer = Some(next_elem.clone());
                            }
                        }
                    }
                }
                self.size -= 1;

                removed.push(Rc::into_inner(element).unwrap().into_inner().request);

            } else {
                // Go to the next one
                pointer = element.borrow().next.clone();
            }
        }

        removed
    }

    fn clear(&mut self) {
        self.head = Rc::new(RefCell::new(QueueElement::default_req()));
        self.tail = Rc::new(RefCell::new(QueueElement::default_req()));
        self.size = 0;
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;

    #[test]
    fn empty_queue() {
        let mut e_queue = Queue::new(10);
        assert_eq!(e_queue.is_empty(), true);
        assert_eq!(e_queue.pop(), None);
    }

    #[test]
    fn push_then_pop() {
        let mut queue = Queue::new(10);
        let req = Request::Cab(8);
        queue.push_unique(req.clone());
        assert_eq!(queue.is_empty(), false);
        assert_eq!(queue.pop(), Some(req));
        assert_eq!(queue.is_empty(), true);
    }

    #[test]
    fn push_peek_pop() {
        let mut queue = Queue::new(10);
        let req = Request::Cab(8);
        queue.push_unique(req.clone());
        assert_eq!(queue.is_empty(), false);
        assert_eq!(queue.peek(), Some(req.clone()));
        assert_eq!(queue.is_empty(), false);
        assert_eq!(queue.pop(), Some(req.clone()));
        assert_eq!(queue.is_empty(), true);
    }

    #[test]
    fn push_push_peek_pop() {
        let mut queue = Queue::new(10);
        let req1 = Request::Cab(8);
        let req2 = Request::Cab(10);

        queue.push_unique(req1.clone());
        assert_eq!(queue.peek(), Some(req1.clone()));
        queue.push_unique(req2.clone());
        assert_eq!(queue.peek(), Some(req1.clone()));
        assert_eq!(queue.pop(), Some(req1.clone()));
        assert_eq!(queue.is_empty(), false);
    }
    #[test]
    fn push_push_retain_one() {
        let mut queue = Queue::new(10);
        let req_retained = Request::Cab(8);
        let req_removed = Request::Cab(10);

        queue.push_unique(req_retained.clone());
        queue.push_unique(req_removed.clone());

        let removed = queue.retain(|x| { x.target() == 8 });
        assert_eq!(removed, vec![req_removed]);
        assert_eq!(queue.peek(), Some(req_retained));
    }

    #[test]
    fn push_push_clear() {
        let mut queue = Queue::new(10);
        let req1 = Request::Cab(8);
        let req2 = Request::Cab(10);

        queue.push_unique(req1.clone());
        queue.push_unique(req2.clone());
        assert_eq!(queue.is_empty(), false);

        queue.clear();
        assert_eq!(queue.is_empty(), true);
        assert_eq!(queue.pop(), None);
    }
}