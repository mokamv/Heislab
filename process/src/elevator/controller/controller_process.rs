use crate::elevator::controller::controller_sync::ControllerSync;
use crate::elevator::controller::elevator_pool::ElevatorPool;
use common::connection::event_handle::controller_handle::controller_handle::ControllerHandle;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use crossbeam_channel::select;
use std::thread::{Builder, JoinHandle};

pub fn start_controller_process_thread(
    controller_handle: Option<ControllerHandle>,
    controller_id: ConnectionIdentifier
) -> JoinHandle<()> {
    let builder = Builder::new().name("Controller".to_string());
    if controller_handle.is_none() { return builder.spawn(|| {}).unwrap() };

    let controller_handle = controller_handle.unwrap();
    let mut controller_sync = ControllerSync::new(controller_id);
    let mut elevator_pool = ElevatorPool::from(
        controller_handle.clients_id()
    );
    
    builder.spawn(move || {
        loop {
            select! {
                recv(controller_sync.recv_takeover_signal()) -> _ => {
                    controller_sync.takeover(
                        &elevator_pool,
                        &controller_handle
                    )
                }

                recv(controller_handle.recv_sync_message()) -> message => {
                    let message = message.unwrap();
                    controller_sync.handle_sync_message(
                        &elevator_pool,
                        &controller_handle,
                        message
                    );
                },

                recv(controller_handle.recv_client_message()) -> message => {
                    let message = message.unwrap();
                    println!("Received from client: {message:?}");
                    elevator_pool.handle_elevator_message(
                        &controller_sync,
                        &controller_handle,
                        message.client_id,
                        message.message
                    );
                }
            }
        }
    }).unwrap()
}