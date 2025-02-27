use crate::process_pair::p_state::ProcessState;
use common::connection::client_pool::client_pool::ClientPool;
use common::connection::connection_init::init_server::{init_controller_tcp_listening, init_udp_broadcasting, init_udp_reconciliation};
use common::connection::controller_state::ControllerState;
use common::log::log_client::Logger;
use common::log::LogLevel;
use std::process::id;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use crossbeam_channel::select;

const MESSAGE_PERIOD: Duration = Duration::from_millis(250);

pub(super) struct Process {
    process_state: ProcessState,
    client_pool: ClientPool,
    logger: Logger,

    process_id: u8,
    faulted: Arc<Mutex<bool>>,
    state: Arc<Mutex<ControllerState>>
}

impl Process {
    pub fn new(id: u8) -> Self {
        let faulted = Arc::new(Mutex::new(false));
        let mut logger = Logger::init(&faulted);

        //TODO PASS THIS AS ARG
        let mut client_pool = ClientPool::new(&faulted, logger.get_sender("[ClientPool]".to_string()), 3);
        client_pool.with_client_id(0)
            .with_client_id(1)
            .with_client_id(2);

        Process {
            process_state: ProcessState::new(),
            state: Arc::new(Mutex::new(ControllerState::BACKUP)),
            process_id: id,
            client_pool: client_pool.start(),
            logger,
            faulted,
        }
    }

    pub fn start_as_backup(mut self) {
        self.logger.send_once(format!("[{}][MAIN] Server has started in backup mode", self.process_id), LogLevel::INFO);

        let tcp_bound_to = init_controller_tcp_listening(
            &self.state,
            &self.faulted,
            &self.client_pool,
            self.logger.get_sender(format!("[{}][Main][TCP]", self.process_id))
        );

        init_udp_broadcasting(
            tcp_bound_to,
            &self.state,
            &self.faulted,
            self.process_id,
            self.logger.get_sender(format!("[{}][MAIN][Broadcast]", self.process_id))
        );

        init_udp_reconciliation(
            &self.state,
            &self.faulted,
            &self.client_pool,
            self.process_id,
            self.logger.get_sender(format!("[{}][MAIN][Broadcast]", self.process_id))
        );

        self.process_task_until_death();

        self.logger.wait_for_logger_termination();
    }

    fn process_task_until_death(&mut self) {
        let logger = self.logger.get_sender(format!("[{}][MAIN]", id()));
        let message_receiver = self.client_pool.take_message_channel();

        while !*self.faulted.lock().unwrap() {
            if *self.state.lock().unwrap() == ControllerState::MASTER {
                select! {
                    recv(message_receiver) -> message => {
                        match message {
                            Ok(message) => {
                                println!("{:?}", message)
                            }
                            Err(error) => break
                        }
                    }
                }
            }
        }

        logger.send("An error occurred\n\n\n", LogLevel::ERROR);
        drop(logger);
        *self.faulted.lock().unwrap() = true;
    }
}