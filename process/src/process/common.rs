use common::connection::client_pool::client_pool::ClientPool;
use common::connection::connection_handle::handle::ConnectionHandle;
use common::connection::connection_init::init_server::{init_controller_tcp_listening, init_udp_broadcasting};
use common::connection::synchronisation::pairing::BackupPairing;
use log::log_client::Logger;
use log::LogLevel;

pub struct Process {
    pub(super) client_handle: ConnectionHandle,
    pub(super) process_id: u8,
}

impl Process {
    pub fn new(process_id: u8) -> Self {
        Logger::init_logger();

        let client_handle = ConnectionHandle::new_client_side_connection_handler(process_id);
        
        Process {
            client_handle,
            process_id,
        }
    }

    fn client_side(mut self) {
        self.client_task();
        Logger::terminate_logging();
    }

    pub fn start_without_controller(process_id: u8) {
        Self::new(process_id).client_side()
    }

    pub fn start_with_controller(process_id: u8, client_count: usize) {
        let mut program = Self::new(process_id);

        Logger::send_once(format!("[{}][MAIN] Server has started in backup mode", program.process_id), LogLevel::INFO);

        let client_pool = ClientPool::new(client_count);

        let backup_pairing = BackupPairing::new(
            program.process_id,
            client_pool.clone(),
        );

        let tcp_bound_to = init_controller_tcp_listening(
            backup_pairing.controller_state_notifier(),
            backup_pairing.controller_link(),
            &client_pool,
            program.process_id
        );

        init_udp_broadcasting(
            tcp_bound_to,
            backup_pairing.controller_state_notifier(),
            program.process_id
        );

        program.controller_task(client_pool.start(), backup_pairing);

        program.client_side();
    }
}