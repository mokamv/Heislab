use std::time::Duration;
use crossbeam_channel::{select, tick, Receiver, RecvError, Sender};
use common::messages::{Message, TimedMessage};
use common::messages::Message::GotoFloor;
use crate::elevator::client::elevator_interaction::ElevatorInteraction;
use crate::process::common::Process;

const FLOOR_COUNT: u8 = 4;

impl Process {
    pub(crate) fn client_task(&mut self) {
        let message_sender = self.client_handle.take_sender();
        let message_receiver = self.client_handle.take_receiver();

        let mut client_state = ClientState {
            identifier: self.process_id,
            elevator_control: ElevatorInteraction::new("127.0.0.1:15657", FLOOR_COUNT),
            is_auth: false,
            message_sender,
            message_receiver,
        };

        let mut floor = 0;
        let ping = tick(Duration::from_millis(50));

        println!("Elevator started");
        loop {
            select! {
                recv(client_state.message_receiver) -> message => {
                    client_state.handle_message(message);
                },
                recv(client_state.elevator_control.event_channel.call_button_rx) -> a => {
                    let call_button = a.unwrap();

                    let pressed_button = match call_button.call {
                        driver_rust::elevio::elev::HALL_UP => PhysicalButton::Hall { direction_is_up: true, floor: call_button.floor },
                        driver_rust::elevio::elev::HALL_DOWN => PhysicalButton::Hall { direction_is_up: false, floor: call_button.floor },
                        driver_rust::elevio::elev::CAB => PhysicalButton::Cab { floor: call_button.floor }
                        _ => panic!("TODO")
                    }
                    
                    client_state.message_sender.send(TimedMessage::of(Message::ClientButtonCall {
                        pressed: pressed_button
                    })).unwrap()
                },
                recv(client_state.elevator_control.event_channel.floor_sensor_rx) -> a => {
                    let floor = a.unwrap();
                    // elevator_controller.state.handle_floor_sensor(floor)
                },
                recv(client_state.elevator_control.event_channel.stop_button_rx) -> a => {
                    let is_pressed = a.unwrap();
                    // elevator_controller.state.handle_stop(is_pressed);
                },
                recv(client_state.elevator_control.event_channel.obstruction_rx) -> a => {
                    let is_obstructed = a.unwrap();
                    // elevator_controller.state.handle_obstruction(is_obstructed);
                },
                recv(client_state.elevator_control.event_channel.close_door_rx) -> _ => {
                    // elevator_controller.state.handle_close_door();
                },
                recv(ping) -> _ => {
                    if client_state.is_auth {
                        client_state.message_sender.send(TimedMessage::of(GotoFloor {go_to_floor: floor})).unwrap();
                        floor += 1;
                    }
                }
            }
        }
    }
}

struct ClientState {
    identifier: u8,
    elevator_control: ElevatorInteraction,
    is_auth: bool,
    message_sender: Sender<TimedMessage>,
    message_receiver: Receiver<Message>
}


impl ClientState {
    pub fn handle_message(&mut self, message: Result<Message, RecvError>) {
        match message {
            Ok(message) => {
                match message {
                    Message::Connected => {
                        self.is_auth = false;
                        println!("Connected to server");
                        self.message_sender.send(TimedMessage::of(Message::ClientAuth { client_id: self.identifier })).unwrap();
                    }
                    Message::Authenticated => {
                        self.is_auth = true;
                        println!("Identified to the server, starting online mode")
                    }

                    Message::Disconnected => {
                        self.is_auth = false;
                        println!("Disconnected from server, starting offline mode");
                    }

                    _ => {}
                }
            }
            Err(_) => { todo!() }
        }
    }
}