use crate::elevator::controller::elevator_pool::ElevatorPool;
use crate::process::common::Process;
use common::connection::client_pool::aggregator::ClientMessage;
use common::connection::client_pool::client_pool::ClientPool;
use common::connection::controller_state::ControllerState;
use common::connection::synchronisation::pairing::BackupPairing;
use crossbeam_channel::select;
use faulted::{is_faulted, set_to_faulted};
use log::LogLevel;
use std::process::id;
use std::thread::spawn;

impl Process {
    pub(super) fn controller_task(&mut self, mut client_pool: ClientPool, backup_pairing: BackupPairing) {
        let logger = self.logger.get_sender(format!("[{}][MAIN]", id()));
        let client_messages = client_pool.take_message_channel();

        spawn(move || {
            let mut elevators_pool = ElevatorPool::from(client_pool);

            while !is_faulted() {
                select! {
                    recv(client_messages) -> message => {
                        if backup_pairing.current_state() == ControllerState::Master {
                            match message {
                                Ok( ClientMessage { identifier, content } ) => {
                                    elevators_pool.handle_elevator_message(identifier,content);
                                }
                                Err(error) => todo!()
                            }
                        }
                    }
                }
            }

            logger.send("An error occurred\n\n\n", LogLevel::ERROR);
            set_to_faulted("Controller task failed");
        });
    }
}
