use crate::elevator::client::elevator_hardware::ElevatorHardware;
use crate::process::common::Process;
use common::connection::connection_handle::message_sender::MessageSender;
use common::data_struct::CabinState;
use common::messages::Message;
use crossbeam_channel::{after, select};
use driver_rust::elevio::elev::{ElevatorEvent, CallType};
use std::time::Duration;

const FLOOR_COUNT: u8 = 4;

impl Process {
    pub(crate) fn client_task(&mut self) {
        let poll_period = Duration::from_millis(25);
        let try_init_after = after(2 * poll_period);

        let mut elevator_hw = ElevatorHardware::new("127.0.0.1:15657", FLOOR_COUNT, poll_period);

        let message_receiver = self.client_handle.take_receiver();
        let (event_receiver, close_door_receiver) = elevator_hw.take_receivers();

        let mut client_state = ClientState {
            identifier: self.process_id,
            elevator_hw,
            is_auth: false,
            message_sender: self.client_handle.take_sender(),
            last_state: Default::default(),
            cab_called: vec![false; FLOOR_COUNT as usize],
        };

        println!("Elevator started");
        loop {
            select! {
                recv(try_init_after) -> _ => {
                    let init_result = client_state.elevator_hw.init_if_is_not_yet();
                },
                recv(message_receiver) -> message => {
                    let message = message.unwrap();
                    client_state.handle_message_event(message);
                },
                recv(event_receiver) -> event => {
                    let event = event.unwrap();
                    client_state.handle_elevator_event(event);
                },
                recv(close_door_receiver) -> _ => {
                    client_state.handle_close_door_event()
                }
            }
        }
    }
}

struct ClientState {
    identifier: u8,
    is_auth: bool,
    elevator_hw: ElevatorHardware,
    message_sender: MessageSender,
    last_state: CabinState,
    cab_called: Vec<bool>,
}


impl ClientState {
    fn handle_message_event(&mut self, message: Message) {
        match message {
            Message::Connected => {
                self.is_auth = false;
                println!("Connected to server");
                self.message_sender.send(Message::ClientAuth { client_id: self.identifier });
            }
            Message::Authenticated => {
                self.is_auth = true;
                self.handle_synchronisation();
                println!("Identified to the server, starting online mode")
            }

            Message::Disconnected => {
                self.is_auth = false;
                println!("Disconnected from server, starting offline mode");
            }

            Message::GotoFloor { go_to_floor } => {
                let new_state = self.elevator_hw.go_to_floor(go_to_floor);
                self.send_updated_state(new_state);
            },

            Message::LightControl { button: target, is_lit } => {
                println!("LIGHT: {target:?}: {is_lit}");
                self.elevator_hw.call_button_light(target, is_lit);
            },

            _ => {}
        }
    }

    fn handle_synchronisation(&mut self) {
        self.message_sender.send(self.last_state);
    }

    fn handle_elevator_event(&mut self, elevator_event: ElevatorEvent) {
        let new_state = self.elevator_hw.handle_event(elevator_event); 
        self.send_updated_state(new_state);
        
        self.update_cab_called_vec(); //TODO

        if self.is_auth {
            self.message_sender.send(elevator_event); 
        } 
    }

    fn handle_close_door_event(&mut self) {
        let new_state = self.elevator_hw.handle_close_door();
        
        if is_auth{
            self.send_updated_state(new_state);
        } else{
            self.handle_newfloor_disconnected(new_state); //is this the only place wee neet to call when diconeccted????????????
        }
        
    }

    fn send_updated_state(&mut self, cabin_state: CabinState) {
        if self.last_state != cabin_state {
            self.last_state = cabin_state;
            if self.is_auth {
                self.message_sender.send(self.last_state);
            }
        }
    }
}


impl ClientState{
    fn update_cab_called_vec(&mut self, event: ElevatorEvent){
        match event { 
            ElevatorEvent::CallButton{ floor, call } => {
                if CallType::Cab == call { 
                    self.cab_called[floor as usize] = true; //OBS: 0-indexed?????????????????????????????
                }    
            }
            _=> { } 
        }
    } 

    fn handle_newfloor_disconnected(&mut self, new_state: CabinState){ 
        match new_state {
            CabinState::DoorOpen { .. } => panic!("Error: Door is open"),
            CabinState::Between { .. } => panic!("Error: Between floors"),
            CabinState::Init => panic!("Error: Init state"),
            CabinState::Idle { current_floor } => {
                for (index, &called) in self.cab_called.iter().enumerate() {
                    if called {
                        self.elevator_hw.go_to_floor(index as u8); //OBS: 0-indexed?????????????????????????????
                        break;
                    }
                }  
            }
        }   
    }
}