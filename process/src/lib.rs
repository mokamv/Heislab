pub mod process {
    pub mod common;
    pub mod client;
    pub mod controller;
}

pub mod elevator {
    pub mod client {
        pub mod elevator_event;
        pub mod elevator_interaction;

        pub mod door_control;
    }
}

pub mod queue {
    pub mod queue;
    pub mod queue_element;
}