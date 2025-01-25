
struct Queue {
    head: Queueelement,
    tail: Queueelement,
    watching_queue_for_stop: bool,
    size: usize,
    limit: usize
}

impl Queue {

    fn new(limit: usize) -> Queue {
        Queue {
            head: Queueelement::new(),
            tail: Queueelement::new(),
            watching_queue_for_stop: false,
            size: 0,
            limit: limit
        }
    }

    fn is_empty(&self) -> bool {
        self.size == 0
    }
    fn pop(&mut self) -> Option<Queueelement> {
        if self.is_empty() {
            return None;
        }
        let element = self.head;
        self.head = element.next;
        self.size -= 1;
        Some(element)
    }

    fn push(&mut self, element: Queueelement) -> bool {
        if self.size == self.limit {
            return false;
        }
        if self.is_empty() {
            self.head = element;
            self.tail = element;
        } else {
            self.tail.next = element;
            self.tail = element;
        }
        self.size += 1;
        true
    }

    fn clear(&mut self) {
        self.head = Queueelement::new();
        self.tail = Queueelement::new();
        self.size = 0;
    }

    fn remove(&mut self, element: Queueelement) -> bool {
        if self.is_empty() {
            return false;
        }
        let mut current = self.head;
        let mut previous = Queueelement::new();
        while current != element {
            if current.next == Queueelement::new() {
                return false;
            }
            previous = current;
            current = current.next;
        }
        if current == self.head {
            self.head = current.next;
        } else {
            previous.next = current.next;
        }
        self.size -= 1;
        true
    }

}