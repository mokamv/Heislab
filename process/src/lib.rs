pub mod process {
    pub mod common;
    pub mod client;
    pub mod controller;
}

pub mod elevator {
    pub mod client {
        pub mod elevator_hardware;

        pub mod door_control;
    }
    
    pub mod controller {
        pub mod light_control;
        pub mod elevator_state;
        pub mod elevator_service;
        pub mod elevator_pool;
    }
}