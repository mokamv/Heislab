use crate::elevator::client::elevator_hardware::ElevatorHardwareState;
use common::connection::event_handle::standalone_handle::standalone_handle::StandaloneHandle;
use common::data_struct::CabinState;
use common::messages::Message;
use crossbeam_channel::{never, select};
use driver_rust::elevio::elev::ElevatorEvent;
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

const FLOOR_COUNT: u8 = 4; // TODO MOVE TO A CONFIG FILE
const POLL_PERIOD: Duration = Duration::from_millis(25); // TODO MOVE TO A CONFIG FILE

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
            FLOOR_COUNT,
            POLL_PERIOD
        ),
    };

    // let tick_debug = tick(Duration::from_millis(250));
    let tick_debug = never::<Instant>();
    let mut a: u8 = 0;

    builder.spawn(move || {
        loop {
            select! {
                recv(tick_debug) -> _ => {
                    if process_state.is_connected_to_controller {
                        a = a.wrapping_add(1);

                        process_state.handle.send_message(
                            Message::GotoFloor {go_to_floor: a}
                        )
                    }
                }

                recv(process_state.handle.recv_message()) -> message => {
                    let message = message.unwrap();
                    process_state.handle_controller_message(message);
                },
                recv(process_state.elevator.recv_native_event()) -> event => {
                    let event = event.unwrap();
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
    fn send_cabin_state_to_controller(&self, state: CabinState) {
        self.handle.send_message(
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
        self.send_cabin_state_to_controller(new_state);

        if self.is_connected_to_controller {
            self.handle.send_message(event.into());
            // TODO ONLINE MODE
        } else {
            // TODO OFFLINE MODE
        }
    }

    fn handle_closed_door_event(&mut self) {
        let new_state = self.elevator.handle_closed_door_event();
        self.send_cabin_state_to_controller(new_state);
    }

    fn handle_synchronisation(&mut self) {
        let current_state = self.elevator.get_current_cabin_state();
        self.send_cabin_state_to_controller(current_state);
    }
}