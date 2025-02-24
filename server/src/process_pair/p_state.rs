pub(super) struct ProcessState {
    counter: u32
}

impl ProcessState {
    pub(super) fn new() -> Self {
        Self { counter: 0 }
    }

    pub(super) fn from(counter: u32) -> Self {
        Self { counter }
    }

    pub(super) fn get_counter(&self) -> u32 {
        self.counter
    }

    pub(super) fn increment_counter_unsafe(&mut self) -> Option<u32> {
        let failed = rand::random_bool(0.0001);

        if failed {
            None
        } else {
            self.counter += 1;
            Some(self.counter)
        }
    }

    pub(super) fn is_older_than(&self, new_value: u32) -> bool {
        self.counter < new_value
    }
}