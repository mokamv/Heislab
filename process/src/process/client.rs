use crate::elevator::client::elevator_hardware::ElevatorHardware;
use crate::process::common::Process;
use common::connection::connection_handle::message_sender::MessageSender;
use common::data_struct::CabinState;
use common::messages::Message;
use crossbeam_channel::{after, select};
use driver_rust::elevio::elev::{ElevatorEvent,MotorDirection};
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
        };

        println!("Elevator started");
        loop {
            select! {
                recv(try_init_after) -> _ => {
                    let init_result = client_state.elevator_hw.init_if_is_not_yet();
                    if let Err(e) = init_result {
                        eprintln!("Error initializing elevator hardware: {:?}", e); //calling the logger instead???????????????
                    }
                },
                recv(message_receiver) -> message => {
                    match message {
                        Ok(message) => client_state.handle_message_event(message),
                        Err(e) => eprintln!("Error receiving message: {:?}", e),
                    }
                },
                recv(event_receiver) -> event => {
                    match event {
                        Ok(event) => client_state.handle_elevator_event(event),
                        Err(e) => eprintln!("Error receiving elevator event: {:?}", e),
                    }
                },
                recv(close_door_receiver) -> event => {
                    match event {
                        Ok(_) => client_state.handle_close_door_event(),
                        Err(e) => eprintln!("Error receiving close door event: {:?}", e),
                    }
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
    mut cab_called: Vec<bool>,
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
    fn update_cab_called_vec(&mut self, event){//TODO
        
        if event == ElevatorEvent::CallButton && event.CallType == Cab { //Cab = 2 in CallType enum
            cab_called[event.floor as usize] = true; //floor takes values from ????????????????????? https://github.com/LeVraiPiroZz/driver-rust
        }    
    }

    handle_newfloor_disconnected(&mut self, new_state: CabinState){ // TODO
        go_to_floor = first true in cab_called (vec<bool>);
        self.elevator_hw.go_to_floor(go_to_floor);

        match CabinState {
            DoorOpen => error,
            Between => error
            DoorClose { current_floor } => //idle
            { 
                loop_index = 0;
                loop {
                    if cab_called[loop_index] == true {
                        self.elevator_hw.go_to_floor(loop_index);
                        break;
                    }
                    loop_index += 1;
                }    
            }
        }   
    }
}