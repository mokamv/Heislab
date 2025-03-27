use crate::elevator::controller::elevator_pool::ElevatorPool;
use common::config::DELAY_TO_BECOME_MASTER;
use common::connection::event_handle::controller_handle::controller_handle::ControllerHandle;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::data_struct::ControllerState;
use common::data_struct::ControllerState::MasterSteppingDown;
use common::messages::Message;
use crossbeam_channel::{after, never, Receiver};
use std::time::Instant;
use ControllerState::{Backup, Master};

pub(in super) struct ControllerSync {
    is_connected: bool,
    controller_id: ConnectionIdentifier,
    controller_state: ControllerState,
    become_master_signal: Receiver<Instant>,
}

impl ControllerSync {
    pub(in super) fn new(controller_id: ConnectionIdentifier) -> Self {
        Self {
            controller_id,
            is_connected: false,
            controller_state: Backup,
            become_master_signal: after(DELAY_TO_BECOME_MASTER),
        }
    }

    pub(in super) fn recv_takeover_signal(&self) -> Receiver<Instant> {
        self.become_master_signal.clone()
    }

    pub(in super) fn takeover(
        &mut self,
        elevator_pool: &ElevatorPool,
        controller_handle: &ControllerHandle
    ) {
        println!("BECOMING MASTER");
        self.update_controller_state(
            controller_handle,
            Master
        );

        let full_matrix = elevator_pool.get_full_requests_matrix();
        controller_handle.send_sync_message(
            Message::ControllerSyncReplace { full_matrix }
        )
    }

    pub(in super) fn handle_sync_message(
        &mut self,
        elevator_pool: &ElevatorPool,
        controller_handle: &ControllerHandle,
        message: Message
    ) {
        println!("MESSAGE FROM OTHER CONTROLLER: {message:?}");
        match message {
            Message::Connected => {
                self.is_connected = true;
                controller_handle.send_sync_message(
                    Message::ControllerSyncState {
                        controller_id: self.controller_id,
                        controller_state: self.controller_state
                    }
                )
            },
            Message::Disconnected => {
                self.is_connected = false;
                if self.controller_state != Master &&
                    // This is a hacky way to now if the current value is never() and not already after(Duration)
                    self.become_master_signal.capacity().unwrap() == 0
                {
                    self.become_master_signal = after(DELAY_TO_BECOME_MASTER);
                }
            },


            Message::ControllerSyncState {
                controller_id: recv_controller_id,
                controller_state: recv_controller_state
            } => self.handle_controller_state_message(
                elevator_pool,
                controller_handle,
                recv_controller_id,
                recv_controller_state
            ),

            Message::ControllerSyncMerge { .. } => self.handle_state_merge(controller_handle),
            Message::ControllerSyncReplace { .. } => self.handle_state_replace(controller_handle),
            Message::ControllerSyncFinish => self.handle_state_finish(controller_handle),

            _ => {}
        }
    }

    fn handle_controller_state_message(
        &mut self,
        elevator_pool: &ElevatorPool,
        controller_handle: &ControllerHandle,
        recv_controller_id: ConnectionIdentifier,
        recv_controller_state: ControllerState
    ) {
        match (self.controller_state, recv_controller_state) {
            // Master and backup get connected, Master send data to replace current backup data.
            (Master, Backup) => {
                let full_matrix = elevator_pool.get_full_requests_matrix();
                controller_handle.send_sync_message(
                    Message::ControllerSyncReplace { full_matrix }
                )
            }
            // Master will be sending replace data, stop timer to become master
            (Backup, Master) => {
                self.become_master_signal = never();
            }
            // Double backup, lower id is going to become master
            (Backup, Backup) => {
                // On such conflict, lower id is prioritized
                if self.controller_id > recv_controller_id {
                    self.become_master_signal = never();
                }
            }
            // Double master, higher id is going to step down and send to-merge data to lower id.
            (Master, Master) => {
                // On such conflict, lower id is prioritized
                // Higher id have to send its current data to be merged before stepping down
                if self.controller_id > recv_controller_id {
                    self.update_controller_state(
                        controller_handle,
                        MasterSteppingDown
                    );
                    let full_matrix = elevator_pool.get_full_requests_matrix();
                    controller_handle.send_sync_message(
                        Message::ControllerSyncMerge { full_matrix }
                    )
                }
            }

            // Somehow, other has become master (or got connected) while this controller is stepping down.
            // Send current data to be merged with the new master.
            (MasterSteppingDown, Master) => {
                let full_matrix = elevator_pool.get_full_requests_matrix();
                controller_handle.send_sync_message(
                    Message::ControllerSyncMerge { full_matrix }
                )
            }
            // Somehow, other has become backup (or got connected) while this controller is stepping down.
            // Cancel stepping down and become master again. (State has been cleared yet)
            (MasterSteppingDown, Backup) => {
                self.update_controller_state(
                    controller_handle,
                    Master
                );
                let full_matrix = elevator_pool.get_full_requests_matrix();
                controller_handle.send_sync_message(
                    Message::ControllerSyncReplace { full_matrix }
                )
            }
            // Both controllers are stepping down, this is unusual.
            // Let higher id controller step-down and makes lower-id step-up
            (MasterSteppingDown, MasterSteppingDown) => {
                // On such conflict, lower id is prioritized
                if self.controller_id < recv_controller_id {
                    self.update_controller_state(
                        controller_handle,
                        Master
                    );
                }
            }
            // Other controller is stepping down, do nothing and wait for the merge-synchronisation payload.
            (Master, MasterSteppingDown) => {}
            // Other is stepping down while this controller is acting as backup.as
            // Do nothing and wait for the other controller to step up again.
            (Backup, MasterSteppingDown) => {}

        }
    }

    fn handle_state_replace(
        &mut self,
        controller_handle: &ControllerHandle
    ) {
        match self.controller_state {
            Backup => { /*TODO HANDLE STATE REPLACE*/ }
            // Stepping down state shouldn't be replaced.
            MasterSteppingDown => {}
            // Master state cannot be replaced.
            Master => {}
        }
    }

    fn handle_state_merge(
        &mut self,
        controller_handle: &ControllerHandle
    ) {
        match self.controller_state {
            // Backup shouldn't be merged
            Backup => {}
            // Stepping down master shouldn't state merged
            MasterSteppingDown => {}
            // Merge state with the master
            Master => {
                //TODO HANDLE MERGING

                // After successful merging, sends ok to other controller so that it can step down.
                controller_handle.send_sync_message(
                    Message::ControllerSyncFinish {}
                )
            }
        }
    }

    fn handle_state_finish(
        &mut self,
        controller_handle: &ControllerHandle
    ) {
        match self.controller_state {
            Backup => {}
            // Only a stepping down master can be acknowledged and be downgraded
            MasterSteppingDown => {
                self.update_controller_state(
                    controller_handle,
                    Backup
                );
            }
            Master => {}
        }
    }

    fn update_controller_state(
        &mut self,
        controller_handle: &ControllerHandle,
        new_controller_state: ControllerState,
    ) {
        if new_controller_state == Master {
            self.become_master_signal = never()
        }

        self.controller_state = new_controller_state;
        controller_handle.send_controller_state(new_controller_state);
        if self.is_connected {
            controller_handle.send_sync_message(
                Message::ControllerSyncState {
                    controller_id: self.controller_id,
                    controller_state: self.controller_state,
                }
            );
        }
    }
}