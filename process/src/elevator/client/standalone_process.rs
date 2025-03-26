use crate::config::{HW_POLL_PERIOD, N_FLOOR};
use crate::elevator::client::elevator_hardware::ElevatorHardwareState;
use common::connection::event_handle::standalone_handle::standalone_handle::StandaloneHandle;
use common::data_struct::CabinState;
use common::messages::Message;
use crossbeam_channel::select;
use driver_rust::elevio::elev::ElevatorEvent;
use std::thread::{Builder, JoinHandle};

struct StandaloneProcessState {
    is_connected_to_controller: bool,
    handle: StandaloneHandle,
    elevator: ElevatorHardwareState
}

pub fn start_standalone_process_thread(standalone_handle: Option<StandaloneHandle>) -> JoinHandle<()> {
    let builder = Builder::new().name("Standalone".to_string());
    if standalone_handle.is_none() { return builder.spawn(|| {}).unwrap() };

    let mut process_state = StandaloneProcessState {
        is_connected_to_controller: false,
        handle: standalone_handle.unwrap(),
        elevator: ElevatorHardwareState::new(
            "127.0.0.1:15657",
            N_FLOOR,
            HW_POLL_PERIOD
        ),
    };

    builder.spawn(move || {
        loop {
            select! {
                recv(process_state.handle.recv_controller_message()) -> message => {
                    let message = message.unwrap();
                    process_state.handle_controller_message(message);
                },
                recv(process_state.elevator.recv_native_event()) -> event => {
                    let event = event.unwrap();
                    println!("EVENT: {event:?}");
                    process_state.handle_native_event(event);
                },
                recv(process_state.elevator.receive_closed_door_event()) -> _ => {
                    process_state.handle_closed_door_event();
                }
            }
        }
    }).unwrap()
}

impl StandaloneProcessState {
    #[inline]
    fn send_cabin_state_to_controller(&self, state: CabinState) {
        self.handle.send_message_to_controller(
            Message::ClientCabinState {
                cabin_state: state,
            }
        )
    }

    fn handle_controller_message(&mut self, message: Message) {
        match message {
            Message::Connected => {
                self.is_connected_to_controller = true;
                self.handle_synchronisation();
                println!("Identified to the server, starting online mode")
            }

            Message::Disconnected => {
                self.is_connected_to_controller = false;
                // TODO MAKE HALL LIGHT FLASH TO INDICATE REFUSAL
                // TODO CHANGE OPERATING MODE
                println!("Disconnected from server, starting offline mode");
            }

            Message::GotoFloor { go_to_floor } => {
                let new_state = self.elevator.set_new_target(go_to_floor);
                self.send_cabin_state_to_controller(new_state);
            },

            Message::LightControl { button: target, is_lit } => {
                println!("LIGHT: {target:?}: {is_lit}");
                self.elevator.set_call_light_state(target, is_lit);
            },

            _ => {
                println!("Message from controller: {message:?}")
            }
        }
    }

    fn handle_native_event(&mut self, event: ElevatorEvent) {
        let new_state = self.elevator.handle_native_event(event);

        if self.is_connected_to_controller {
            self.send_cabin_state_to_controller(new_state);
            self.handle.send_message_to_controller(event.into());
        } else {
            if let ElevatorEvent::CallButton {..} = event {
                self.elevator.offline_handle_next_cab_call();
            }
        }
    }

    fn handle_closed_door_event(&mut self) {
        let new_state = self.elevator.handle_closed_door_event();
        if self.is_connected_to_controller {
            // If is connected, let the controller handle the situation
            self.send_cabin_state_to_controller(new_state);
        } else {
            // If is disconnected.
            // Look into the current cabin pressed list and serve the nearest floor, priority to up.
            // TODO THIS IS DEEPLY UNOPTIMIZED AND INEFFICIENT, THIS WILL HAVE TO CHANGE
            // TODO Implement an algorithm to keep the same direction
            self.elevator.offline_handle_next_cab_call();
        }
    }

    fn handle_synchronisation(&mut self) {
        let current_state = self.elevator.get_current_cabin_state();
        self.send_cabin_state_to_controller(current_state);
    }
}