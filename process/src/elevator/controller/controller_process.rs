use crate::elevator::controller::controller_sync::ControllerSync;
use common::connection::event_handle::controller_handle::controller_handle::{ClientMessage, ControllerHandle};
use crossbeam_channel::select;
use std::thread::{Builder, JoinHandle};
use common::connection::event_handle::handle_state::ConnectionIdentifier;

pub fn start_controller_process_thread(
    controller_handle: Option<ControllerHandle>,
    controller_id: ConnectionIdentifier
) -> JoinHandle<()> {
    let builder = Builder::new().name("Controller".to_string());
    if controller_handle.is_none() { return builder.spawn(|| {}).unwrap() };

    let mut controller_handle = controller_handle.unwrap();
    let mut controller_sync = ControllerSync::new(controller_id);
    
    builder.spawn(move || {
        loop {
            select! {
                recv(controller_sync.recv_takeover_signal()) -> _ => {
                    controller_sync.takeover(
                        &controller_handle
                    )
                }

                recv(controller_handle.recv_sync_message()) -> message => {
                    let message = message.unwrap();
                    controller_sync.handle_sync_message(
                        &controller_handle,
                        message
                    );
                },

                recv(controller_handle.recv_client_message()) -> message => {
                    let message = message.unwrap();
                    handle_client_message(message);
                }
            }
        }
    }).unwrap()
}

fn handle_client_message(message: ClientMessage) {
    // TODO
    // let client_id = message.client_id;
    // let message = message.message;
    //
    // println!("MESSAGE FROM CLIENT {client_id}: {message:?}");
    //
    // match message {
    //     Message::Connected => {
    //         self.is_connected_to_client = true;
    //         self.handle.send_client_message(
    //             Target::Specific(client_id),
    //             Message::ClientCabinState {
    //                 cabin_state: Default::default(),
    //             }
    //         )
    //     },
    //     Message::Disconnected => self.is_connected_to_client = false,
    //     _ => {}
    // }
}