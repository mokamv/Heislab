use common::connection::client_pool::client_pool::ClientPool;
use common::connection::connection_handle::handle::ConnectionHandle;
use common::connection::connection_init::init_server::{init_controller_tcp_listening, init_udp_broadcasting};
use common::connection::synchronisation::pairing::BackupPairing;
use log::log_client::Logger;
use log::LogLevel;

pub struct Process {
    pub(super) client_handle: ConnectionHandle,
    pub(super) process_id: u8,
    pub(super) logger: Logger,
}

impl Process {
    pub fn new(id: u8) -> Self {
        let mut logger = Logger::init();

        let client_handle = ConnectionHandle::new_client_connection_handler(
            logger.get_sender(format!("[Client][{id}]"))
        );
        
        Process {
            client_handle,
            process_id: id,
            logger,
        }
    }

    fn client_side(mut self) {
        self.client_task();
        self.logger.wait_for_logger_termination();
    }

    pub fn start_without_controller(id: u8) {
        Self::new(id).client_side()
    }

    pub fn start_with_controller(id: u8, client_count: usize) {
        let mut program = Self::new(id);

        program.logger.send_once(format!("[{}][MAIN] Server has started in backup mode", program.process_id), LogLevel::INFO);

        let client_pool = ClientPool::new(
            program.logger.get_sender("[ClientPool]".to_string()),
            client_count
        );

        let backup_pairing = BackupPairing::new(
            id,
            client_pool.clone(),
            program.logger.get_sender(format!("[Controller][{id}]")),
        );

        let tcp_bound_to = init_controller_tcp_listening(
            backup_pairing.controller_state_notifier(),
            backup_pairing.controller_link(),
            &client_pool,
            program.logger.get_sender(format!("[{}][Main][TCP]", program.process_id))
        );

        init_udp_broadcasting(
            tcp_bound_to,
            backup_pairing.controller_state_notifier(),
            program.process_id,
            program.logger.get_sender(format!("[{}][MAIN][Broadcast]", program.process_id))
        );

        program.controller_task(client_pool.start(), backup_pairing);

        program.client_side();
    }
}