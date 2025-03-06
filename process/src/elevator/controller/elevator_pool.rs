use common::connection::client_pool::client_pool::{ClientPool, Target};
use common::connection::connection_handle::handle::ConnectionIdentifier;
use common::data_struct::{CabinState, CallRequest};
use common::messages::Message;
use crate::elevator::controller::elevator_state::ElevatorState;

struct IdentifiedElevator {
    elevator: ElevatorState,
    identifier: ConnectionIdentifier
}

pub struct ElevatorPool {
    pool: Vec<IdentifiedElevator>,
    client_pool: ClientPool
}

impl ElevatorPool {
    pub fn from(client_pool: ClientPool) -> Self {
        let mut elevators: Vec<IdentifiedElevator> = vec![];
        client_pool.client_identifiers().iter().for_each(|elevator_id| {
            elevators.push(IdentifiedElevator { elevator: Default::default(), identifier: *elevator_id })
        });

        Self {
            pool: elevators,
            client_pool,
        }
    }

    fn get_elevator(&mut self, elevator_id: ConnectionIdentifier) -> &mut ElevatorState {
        &mut self.pool.iter_mut().find(|candidate| candidate.identifier == elevator_id).unwrap().elevator
    }

    pub fn handle_elevator_message(
        &mut self,
        identifier: ConnectionIdentifier,
        message: Message
    ) {
        let elevator = self.get_elevator(identifier);
        println!("{message:?}");

        match message {
            Message::Connected => {
                elevator.set_connected(false);
                println!("Connected to a client");
            }
            Message::Authenticated => {
                elevator.set_connected(true);
                println!("Identified to a client")
            }

            Message::Disconnected => {
                elevator.set_connected(false);
                println!("Disconnected from client");
            }

            Message::ClientButtonCall { pressed } => {
                //TODO: make this as a memberfunction of elevatorPool  
                match pressed{
                    Hall => {
                        mut min cost = 128;
                        mut min elevator_min_cost;
                        for identifiedEelev in self.poll{
                            elev_cost = identifiedEelevator.elevator.cost()
                            if elev_cost < cost{
                                cost = elev_cost
                                elevator_min_cost = elevator; 
                            }
                            identifiedEelevator.elevator.turn_on_lights(message); //TODO: function turn on lights not made, must send a message to client
                        }
                        //turning on lights
                        self.client_pool.send(
                            id = identifiedEelevator.identifier;
                            Target::Specific(id),
                            Message::LightControl { target: pressed, is_lit: true }
                        ).unwrap();
                    }
                    Cab => {
                        message.identifier
                        //add to the respective elevator queue
                        self.client_pool.send(
                            Target::Specific(identifier),
                            Message::GotoFloor { go_to_floor: pressef.target()}
                        ).unwrap();
                    }
                };
            }

            Message::ClientObstructed { .. } => {}

            //TODO CHANGE
            Message::ClientCabinState { cabin_state } => {
                match cabin_state {
                    CabinState::DoorOpen { current_floor } => {
                        self.client_pool.send(
                            Target::Specific(identifier),
                            Message::LightControl { target: CallRequest::Cab { floor: current_floor } , is_lit: false }
                        ).unwrap();
                    }
                    CabinState::DoorClose { .. } => {}
                    CabinState::Between { .. } => {}
                }
            }


            Message::ClientStopButton { .. } => println!("Unimplemented"),



            Message::ControllerAddress { .. } => {}
            Message::LightControl { .. } => {}
            Message::GotoFloor { .. } => {}
            Message::ClientAuth { .. } => {}
            Message::ControllerAuth { .. } => {}
            Message::ControllerCurrentState { .. } => {}

            Message::KeepAlive => unreachable!(),
        }
    }

}