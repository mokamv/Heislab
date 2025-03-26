
pub mod elevator {
    pub mod client {
        pub mod standalone_process;
        pub mod elevator_hardware;
        pub mod door_control;
    }
    
    pub mod controller {
        pub mod controller_process;
        pub mod controller_sync;
        pub mod light_control;
        pub mod elevator_state;
        pub mod elevator_service;
        pub mod elevator_pool;
    }
}