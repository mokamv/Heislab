use crate::process_pair::process::Process;
use common::log::{log_server, LogLevel};

mod process;
mod p_state;
mod p_message;

pub struct ProcessPair {}

impl ProcessPair {
    pub fn run_as_program(id: u8) {
        let program = Process::new(id);
        program.start_as_backup();
    }

    pub fn run_as_overview(log_level: LogLevel) {
        // Start log server, this is blocking.
        if let Err(error) = log_server::act_as_primary_logger(log_level) {
            println!("Log server encountered an error: {:?}", error)
        }
    }
}