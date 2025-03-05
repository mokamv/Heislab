use crate::connection::client_pool::client_pool::ClientPool;
use crate::connection::controller_state::ControllerState::{Backup, Master, MasterSteppingDown};
use crate::connection::controller_state::{ControllerState, ControllerStateNotifier};
use crate::connection::synchronisation::controller_link::ControllerLink;
use crate::messages::Message::{ControllerAuth, ControllerCurrentState};
use crate::messages::Message;
use crossbeam_channel::{select_biased, Receiver};
use log::log_client::ReliableLogSender;
use std::cmp::Ordering;
use std::thread::spawn;

pub struct BackupPairing {
    controller_link: ControllerLink,
    controller_state: ControllerStateNotifier,
}

impl BackupPairing {
    pub fn new(
        controller_id: u8,
        client_pool: ClientPool,
        logger: ReliableLogSender
    ) -> Self {
        let (controller_state, controller_state_recv)
            = ControllerStateNotifier::new(
            controller_id,
            logger.clone()
        );

        let controller_link = ControllerLink::new(
            controller_state.clone(),
            client_pool,
            logger
        );

        let mut process = Self { controller_link, controller_state };

        process.handle_controller_sync(controller_state_recv);

        process
    }

    pub fn controller_state_notifier(&self) -> ControllerStateNotifier {
        self.controller_state.clone()
    }

    pub fn controller_link(&self) -> ControllerLink {
        self.controller_link.clone()
    }

    // pub fn send_full_sync(controller_sync_message_sender: &Sender<Message>) {
    //     controller_sync_message_sender.send()
    // }


    //TODO CHANGE
    pub fn current_state(&self) -> ControllerState {
        self.controller_state.current_state()
    }

    fn handle_controller_sync(&mut self, controller_state_receiver: Receiver<ControllerState>) {
        let controller_sync_message_receiver = self.controller_link.take_receiver();
        let controller_sync_message_sender = self.controller_link.take_sender();

        let controller_state = self.controller_state_notifier();

        let mut current_state = self.controller_state.current_state();
        // let mut is_connected_to_other = false;

        spawn(move || {
            'sync_loop: loop {
                select_biased! {
                    recv(controller_state_receiver) -> new_state => {
                        match new_state {
                            Ok(new_state) => {
                                current_state = new_state;
                                controller_sync_message_sender.send(
                                    ControllerCurrentState {
                                        id: controller_state.controller_id(),
                                        state: current_state
                                    }
                                );

                                //TODO BELOW
                                match new_state {
                                    Backup => {} // No important action to trigger
                                    MasterSteppingDown
                                    | Master => {}
                                }
                                // TODO ABOVE
                            }
                            Err(_error) => break 'sync_loop
                        }
                    },
                    recv(controller_sync_message_receiver) -> controller_message => {
                        match controller_message {
                            Ok(controller_message) => {
                                match controller_message {
                                    Message::Connected => {
                                        controller_sync_message_sender.send(ControllerAuth {
                                                controller_id: controller_state.controller_id()
                                            }
                                        )
                                    },
                                    Message::Disconnected => {println!("Disconnected")}, //TODO
                                    Message::Authenticated => {
                                        controller_sync_message_sender.send(ControllerCurrentState {
                                                id: controller_state.controller_id(),
                                                state: current_state
                                            }
                                        )
                                    }

                                    ControllerCurrentState { state, id } => {
                                        Self::handle_controller_state(&controller_state, &current_state, state, id)
                                    }
                                    _ => {}
                                }

                                // TODO HANDLE MESSAGE HERE
                            }
                            Err(_error) => break 'sync_loop
                        }
                    }
                }
            }
        });
    }

    fn handle_controller_state(
        controller_state_notifier: &ControllerStateNotifier,
        current_controller_state: &ControllerState,
        other_state: ControllerState,
        other_id: u8
    ) {
        match (current_controller_state, other_state) {
            // Ignore those situations.
            (Backup, Master) | (MasterSteppingDown, Master) |
            (Master, Backup) | (Master, MasterSteppingDown) => {},

            // Both are soon-to-be backup or are already.
            // Update the soon-to-be controller so that it become controller immediately.
            | (Backup, Backup) | (Backup, MasterSteppingDown)
            | (MasterSteppingDown, Backup) | (MasterSteppingDown, MasterSteppingDown) => {
                match controller_state_notifier.controller_id().cmp(&other_id) {
                    Ordering::Equal => panic!("Self message isn't possible over TCP"),
                    // This one is suitable to become master
                    Ordering::Less => controller_state_notifier.change_state(Master),
                    // Stay a backup
                    Ordering::Greater => {}
                }
            },

            // Double master situation, we need reconciliation
            (Master, Master) => {
                match controller_state_notifier.controller_id().cmp(&other_id) {
                    Ordering::Equal => panic!("Self message isn't possible over TCP"),
                    // Stay master.
                    Ordering::Less => {}
                    // Prepare to become backup
                    Ordering::Greater => controller_state_notifier.change_state(MasterSteppingDown)
                }
            }
        }
    }
}