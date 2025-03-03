use std::process::id;
use std::thread::spawn;
use crossbeam_channel::select;
use common::connection::client_pool::client_pool::ClientPool;
use common::connection::controller_state::ControllerState;
use common::connection::synchronisation::pairing::BackupPairing;
use log::LogLevel;
use crate::process::common::Process;

impl Process {
    pub(super) fn controller_task(&mut self, mut client_pool: ClientPool, backup_pairing: BackupPairing) {
        let logger = self.logger.get_sender(format!("[{}][MAIN]", id()));
        let client_messages = client_pool.take_message_channel();
        let faulted = self.faulted.clone();

        spawn(move || {
            while !*faulted.lock().unwrap() {
                select! {
                    recv(client_messages) -> message => {
                        if backup_pairing.current_state() == ControllerState::Master {
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
            *faulted.lock().unwrap() = true;
        });
    }
}

