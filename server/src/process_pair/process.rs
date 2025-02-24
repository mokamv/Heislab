use crate::process_pair::p_message::backup_read_stdin_loop;
use crate::process_pair::p_start::{launch_process, ProcessType};
use crate::process_pair::p_state::ProcessState;
use std::io::Write;
use std::process::id;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use common::connection::client_pool::client_pool::ClientPool;
use common::connection::connection_init::init_server::{init_server_tcp_listening, init_server_udp_broadcasting};
use common::log::log_client::Logger;
use common::log::LogLevel;

const MESSAGE_PERIOD: Duration = Duration::from_millis(250);

pub(super) struct Process {
    process_state: ProcessState,
    client_pool: ClientPool,
    logger: Logger,
    faulted: Arc<Mutex<bool>>
}

impl Process {
    pub fn new() -> Self {
        let faulted = Arc::new(Mutex::new(false));
        let mut logger = Logger::init(&faulted);

        //TODO PASS THIS AS ARG
        let mut client_pool = ClientPool::init(&faulted, logger.get_sender("[ClientPool]".to_string()), 3);
        client_pool.with_client_id(0)
            .with_client_id(1)
            .with_client_id(2);

        Process {
            process_state: ProcessState::new(),
            client_pool,
            logger,
            faulted
        }
    }

    pub fn from(process_state: ProcessState) -> Self {
        let mut process = Self::new();
        process.process_state = process_state;
        process
    }

    pub fn listen_until_main_dies(mut self) -> Process {
        let stdin_receiver = backup_read_stdin_loop();
        let mut iteration_since_last_response = 0u8;
        loop {
            match stdin_receiver.try_recv() {
                Ok(new_value) => {
                    if self.process_state.is_older_than(new_value) {
                        self.process_state = ProcessState::from(new_value);
                        iteration_since_last_response = 0;
                    }
                }

                // Handle error
                Err(error) => {
                    match error {
                        TryRecvError::Empty => {
                            iteration_since_last_response += 1;
                            if iteration_since_last_response > 5 {
                                drop(stdin_receiver);
                                break
                            }
                        }


                        TryRecvError::Disconnected => {
                            drop(stdin_receiver);
                            break;
                        }
                    }
                }
            };
            sleep(MESSAGE_PERIOD);
        }

        self
    }

    pub fn takeover(mut self) {
        self.logger.send_once(format!("[{}][MAIN] Begin taking over", id()), LogLevel::INFO);

        init_server_udp_broadcasting(
            &self.faulted,
            self.logger.get_sender(format!("[{}][MAIN][Broadcast]", id()))
        );

        init_server_tcp_listening(
            &self.faulted,
            &self.client_pool,
            self.logger.get_sender(format!("[{}][Main][TCP]", id()))
        );

        self.process_task();
    }

    fn process_task(mut self) {
        let logger = self.logger.get_sender(format!("[{}][MAIN]", id()));

        'program_loop: loop {

            let mut backup_stdin
                = launch_process(ProcessType::BACKUP(self.process_state.get_counter())).stdin.take().unwrap();

            'backup_is_alive: loop {
                let faulted_lock = self.faulted.lock().unwrap();
                if *faulted_lock { break 'program_loop }

                match self.process_state.increment_counter_unsafe() {
                    None => { 
                        logger.send("PROGRAM CRASHED NORMALLY", LogLevel::INFO);
                        break 'program_loop },
                    Some(produced_value) => {
                        match backup_stdin.write(&produced_value.to_be_bytes()) {
                            Ok(0) | Err(_) => {
                                break 'backup_is_alive
                            }
                            Ok(_) => {}
                        }

                        logger.send(&format!("counter is {}", produced_value), LogLevel::INFO);
                    }
                };

                drop(faulted_lock);
                sleep(MESSAGE_PERIOD);
            }

            drop(backup_stdin);
            sleep(5 * MESSAGE_PERIOD);
        }

        logger.send("An error occurred\n\n\n", LogLevel::ERROR);
        drop(logger);
        *self.faulted.lock().unwrap() = true;

        self.logger.wait_for_logger_termination();
    }
}