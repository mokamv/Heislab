use common::connection::client_pool::client_pool::ClientPool;
use common::connection::connection_init::init_server::{init_controller_tcp_listening, init_udp_broadcasting};
use common::connection::synchronisation::pairing::BackupPairing;
use log::log_client::Logger;
use log::LogLevel;
use std::sync::{Arc, Mutex};
use common::connection::connection_handle::handle::ConnectionHandle;

pub struct Process {
    pub(super) client_handle: ConnectionHandle,
    pub(super) process_id: u8,
    pub(super) logger: Logger,
    pub(super) faulted: Arc<Mutex<bool>>,
}

impl Process {
    pub fn new(id: u8) -> Self {
        let faulted = Arc::new(Mutex::new(false));
        let mut logger = Logger::init(&faulted);

        let client_handle = ConnectionHandle::new_client_connection_handler(
            logger.get_sender(format!("[Client][{id}]")),
            &faulted
        );
        
        Process {
            client_handle,
            process_id: id,
            logger,
            faulted,
        }
    }

    fn client_side(mut self) {
        //TODO
        self.client_task();
        self.logger.wait_for_logger_termination();
    }

    pub fn start_without_controller(id: u8) {
        let mut program = Self::new(id);
        program.client_side()
    }

    pub fn start_with_controller(id: u8, client_count: usize) {
        let mut program = Self::new(id);

        program.logger.send_once(format!("[{}][MAIN] Server has started in backup mode", program.process_id), LogLevel::INFO);

        let client_pool = ClientPool::new(
            &program.faulted,
            program.logger.get_sender("[ClientPool]".to_string()),
            client_count
        );

        let backup_pairing = BackupPairing::new(
            id,
            client_pool.clone(),
            program.logger.get_sender(format!("[Controller][{id}]")),
            &program.faulted
        );

        let tcp_bound_to = init_controller_tcp_listening(
            backup_pairing.controller_state_notifier(),
            backup_pairing.controller_link(),
            &program.faulted,
            &client_pool,
            program.logger.get_sender(format!("[{}][Main][TCP]", program.process_id))
        );

        init_udp_broadcasting(
            tcp_bound_to,
            backup_pairing.controller_state_notifier(),
            &program.faulted,
            program.process_id,
            program.logger.get_sender(format!("[{}][MAIN][Broadcast]", program.process_id))
        );

        program.controller_task(client_pool.start(), backup_pairing);

        program.client_side();
    }
}