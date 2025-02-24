use crate::process_pair::p_start::{launch_process, ProcessType};
use crate::process_pair::p_state::ProcessState;
use crate::process_pair::process::Process;
use common::log::{log_server, LogLevel};

mod process;
mod p_start;
mod p_state;
mod p_message;

pub struct ProcessPair {}

impl ProcessPair {

    pub fn run_as_main() {
        Process::new().takeover();
    }

    pub fn run_as_backup(counter: u32) {
        let backup = Process::from(ProcessState::from(counter));

        let main = backup.listen_until_main_dies();

        main.takeover();
    }

    pub fn run_as_overview(log_level: LogLevel) {
        // Launch main process.
        launch_process(ProcessType::MAIN);

        // Start log server, this is blocking.
        if let Err(error) = log_server::act_as_primary_logger(log_level) {
            println!("Log server encountered an error: {:?}", error)
        }
    }
}