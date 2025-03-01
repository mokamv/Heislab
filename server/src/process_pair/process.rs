use common::connection::client_pool::client_pool::ClientPool;
use common::connection::connection_init::init_server::{init_controller_tcp_listening, init_udp_broadcasting};
use common::connection::controller_state::ControllerState;
use common::connection::synchronisation::pairing::BackupPairing;
use common::log::log_client::Logger;
use common::log::LogLevel;
use crossbeam_channel::select;
use std::process::id;
use std::sync::{Arc, Mutex};

pub(super) struct Process {
    backup_pairing: BackupPairing,

    client_pool: ClientPool,
    logger: Logger,

    process_id: u8,
    faulted: Arc<Mutex<bool>>,
}

impl Process {
    pub fn new(id: u8, client_pool_size: usize) -> Self {
        let faulted = Arc::new(Mutex::new(false));
        let mut logger = Logger::init(&faulted);

        let client_pool
            = ClientPool::new(&faulted, logger.get_sender("[ClientPool]".to_string()), client_pool_size);

        let backup_pairing = BackupPairing::new(
            id,
            client_pool.clone(),
            logger.get_sender(format!("[Controller][{id}]")),
            &faulted
        );

        Process {
            backup_pairing,
            process_id: id,
            client_pool: client_pool.start(),
            logger,
            faulted,
        }
    }

    pub fn start_as_backup(id: u8, client_count: usize) {
        let mut program = Self::new(id, client_count);

        program.logger.send_once(format!("[{}][MAIN] Server has started in backup mode", program.process_id), LogLevel::INFO);

        let tcp_bound_to = init_controller_tcp_listening(
            program.backup_pairing.controller_state_notifier(),
            program.backup_pairing.controller_link(),
            &program.faulted,
            &program.client_pool,
            program.logger.get_sender(format!("[{}][Main][TCP]", program.process_id))
        );

        init_udp_broadcasting(
            tcp_bound_to,
            program.backup_pairing.controller_state_notifier(),
            &program.faulted,
            program.process_id,
            program.logger.get_sender(format!("[{}][MAIN][Broadcast]", program.process_id))
        );

        program.process_task_until_death();

        program.logger.wait_for_logger_termination();
    }

    fn process_task_until_death(&mut self) {
        let logger = self.logger.get_sender(format!("[{}][MAIN]", id()));
        let client_messages = self.client_pool.take_message_channel();

        while !*self.faulted.lock().unwrap() {
            select! {
                recv(client_messages) -> message => {
                    if self.backup_pairing.current_state() == ControllerState::Master {
                        match message {
                            Ok(message) => {
                                println!("{:?}", message)
                            }
                            Err(error) => todo!()
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