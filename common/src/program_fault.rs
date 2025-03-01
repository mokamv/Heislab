use std::sync::{Arc, Mutex};

pub type Faulted = Arc<Mutex<bool>>;

pub fn program_set_to_faulted(faulted_state: &Faulted, reason: &str) {
    *faulted_state.lock().unwrap() = true;
    if cfg!(debug_assertions) {
        panic!("{}", reason)
    } else {
        println!("{}", reason)
    }
}