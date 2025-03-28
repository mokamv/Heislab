use crate::elevator::client::door_control::DoorControl;
use driver_rust::elevio::elev::{CallType, Elevator, ElevatorEvent, FloorEvent, MotorDirection};
use std::time::Duration;
use crossbeam_channel::Receiver;
use driver_rust::elevio::elev::FloorEvent::{AtFloor, BetweenFloors};
use common::config::N_FLOOR;
use common::data_structures::cabin_state::CabinState;
use common::data_structures::call_request::CallRequest;

pub struct MinimalState {
    cab_called: [bool; N_FLOOR as usize],
    cabin: CabinState,
    target: Option<u8>,
    last_direction: MotorDirection,
}

pub struct ElevatorHardwareState {
    state: MinimalState,
    elevator: Elevator,
    door_control: DoorControl,
}

impl ElevatorHardwareState {
    /// Create a new [ElevatorHardwareState] instance with the given address, floor count and poll period.
    pub fn new(addr: &str, floor_count: u8, poll_period: Duration) -> Self {
        let door_control = DoorControl::new(poll_period);
        let mut elevator = Elevator::init(addr, floor_count)
            .expect("Cannot connect to elevator hardware");
        elevator.event_loop(poll_period);

        Self {
            door_control,
            state: MinimalState {
                cab_called: [false; N_FLOOR as usize],
                target: None,
                cabin: Default::default(),
                last_direction: MotorDirection::Stop,
            },
            elevator,
        }
    }

    pub fn recv_native_event(&self) -> &Receiver<ElevatorEvent> {
        &self.elevator.event_receiver
    }

    pub fn receive_closed_door_event(&self) -> &Receiver<()> {
        &self.door_control.recv_closed_door_event()
    }
}

impl ElevatorHardwareState {

    /// Set the target floor for the elevator to reach.
    /// The function will return the new state of the elevator cabin.
    pub fn set_new_target(&mut self, target: u8) -> CabinState {
        match self.state.cabin {
            // If the door is open, the elevator is not allowed to move
            CabinState::DoorOpen { .. } => {
                println!("Go to floor cannot be called while doors are opened");
                // return the current state
                return self.state.cabin
            },
            // If the elevator is idle or between two floors, set the target and start moving in the right direction
            CabinState::Idle { .. }
            | CabinState::Between { .. } => {
                self.state.target = Some(target);

                let from_floor = self.state.cabin.get_current_floor_relative_to(target);
                let direction = self.state.cabin.get_direction_relative_to(target);

                // Calculate the next floor to reach
                let to_floor = match direction {
                    MotorDirection::Stop => return self.reach_target(),
                    MotorDirection::Down => from_floor - 1,
                    MotorDirection::Up => from_floor + 1
                };

                // Update the state and direction of the elevator cabin,
                // and return its new state
                self.state.cabin = CabinState::Between { from_floor, to_floor };
                self.set_motor_direction(direction);
                self.state.cabin
            }
            CabinState::Init => unreachable!("Function go_to_floor cannot be called while elevator is initializing")
        }
    }

    /// Handle cab calls when the elevator is not connected to the controller (single elevator mode)
    /// with respect to the current state of the elevator.
    pub fn offline_handle_next_cab_call(&mut self) {
        match self.state.cabin {
            CabinState::DoorOpen { .. } => return,
            CabinState::Idle { current_floor } => self.find_closest_call(current_floor, None),
            CabinState::Between { from_floor, to_floor } => self.find_closest_call(from_floor, Some(to_floor)),
            CabinState::Init => return,
        }
    }

    /// Used when receiving cab call in offline mode. 
    /// Set target to the closest call in motordirection, 
    /// and stops elevator if no cab calls
    pub fn find_closest_call(&mut self, last_floor: u8, to_floor: Option<u8>) {
        let motor_direction = self.state.cabin.get_direction();
        println!("Last floor: {}", last_floor);
        println!("Last direction:{:?}", self.state.last_direction);

        //iterating called floors in the cab_called vector
        let next_call: Option<(u8, u8)> = self.state
            .cab_called
            .iter()
            .enumerate()
            .filter_map(|(index, is_called)| {

                let target_floor = index as u8;
                let target_direction = self.state.cabin.get_direction_relative_to(target_floor);
                
                //for each floor calculating the distance to the target
                if *is_called &&
                    ((motor_direction == MotorDirection::Up && target_floor > last_floor) ||
                    (motor_direction == MotorDirection::Down && target_floor < last_floor) ||
                    (motor_direction == MotorDirection::Stop ))
                {   
                    if target_floor == last_floor {
                        Some((target_floor, 0))
                    } else if target_direction != self.state.last_direction {
                        //setting low priority to call not in motordirection by adding N_FLOOR to the distace
                        Some((target_floor, u8::abs_diff(last_floor, target_floor) + N_FLOOR))
                    } else {
                        Some((target_floor, u8::abs_diff(last_floor, target_floor)))
                    }
                } else {
                    None
                }
            })
            //setting the next call to the target with the shortest distance
            .min_by(|(_, d1), (_, d2)| {d1.cmp(d2)});
            
        println!("NEXT_CALL: {next_call:?}");

        if let Some((new_target, _)) = next_call {
            self.set_new_target(new_target);
        } else {
            self.set_motor_direction(MotorDirection::Stop);
        }
    }

    /// Initialize the elevator if it is not yet initialized.
    pub fn init_if_is_not_yet(&mut self) {
        if self.state.cabin == CabinState::Init {
            // Set the elevator to move down until it reaches a floor
            self.set_motor_direction(MotorDirection::Down);
        }
    }

    pub fn set_call_light_state(&mut self, call_request: CallRequest, on: bool) {
        let floor = call_request.target();
        self.elevator.call_button_light(floor, call_request.into(), on);
    }

    pub fn set_all_call_lights_state(&mut self, on: bool) {
        // Loop through all floors and set the call lights
        for floor in 0..N_FLOOR {
            self.elevator.call_button_light(floor, CallType::Cab, on);
            self.elevator.call_button_light(floor, CallType::HallUp, on);
            self.elevator.call_button_light(floor, CallType::HallDown, on);
        }
    }

    pub fn set_hall_lights_state(&mut self, on: bool) {
        // Loop through all floors and set the hall lights
        for floor in 0..N_FLOOR {
            self.elevator.call_button_light(floor, CallType::HallUp, on);
            self.elevator.call_button_light(floor, CallType::HallDown, on);
        }
    }

    pub fn set_emergency_light_state(&mut self, on: bool) {
        self.elevator.stop_button_light(on);
    }

    pub fn set_cab_pressed(&mut self, cab_pressed: [bool; N_FLOOR as usize]) {
        self.state.cab_called = cab_pressed
    }

    pub fn get_cab_pressed(&self) -> [bool; N_FLOOR as usize] {
        self.state.cab_called
    }

    pub fn get_current_cabin_state(&self) -> CabinState {
        self.state.cabin
    }

    pub fn get_obstruction(&self) -> bool {
        self.door_control.is_obstructed()
    }
}

impl ElevatorHardwareState {
    /// Handle the event of the elevator reaching its target floor.
    /// This function will set the motor direction to stop, open the door, and turn off the call light.
    /// The function will return the new state of the elevator cabin.
    fn reach_target(&mut self) -> CabinState {
        self.set_motor_direction(MotorDirection::Stop);
        // Open the door
        self.elevator.door_light(true);
        self.door_control.open_door();

        let floor_reached = self.state.target.unwrap();
        self.elevator.call_button_light(floor_reached, CallType::Cab, false);
        self.state.cab_called[floor_reached as usize] = false;
        self.state.cabin = CabinState::DoorOpen { current_floor: floor_reached };
        self.state.target = None;
        self.state.cabin
    }

    /// Handle the event of the elevator reaching a floor and does not have a target floor.
    /// The function will set the motor direction to stop and return the new state of the elevator cabin.
    fn reach_idle(&mut self, floor: u8) -> CabinState {
        self.set_motor_direction(MotorDirection::Stop);
        self.state.target = None;
        self.state.cabin = CabinState::Idle { current_floor: floor };
        self.state.cabin
    }

    /// Handle the event of the elevator reaching a floor that is not the target floor.
    /// The function will increment the values inside the [Between] state and return the new state of the elevator cabin.
    fn reach_non_target_floor(&mut self) -> CabinState {
        self.state.cabin.increment_between();
        self.state.cabin
    }
}

impl ElevatorHardwareState {
    /// Handle the native event of the elevator.
    pub fn handle_native_event(&mut self, event: ElevatorEvent) -> CabinState {
        match event {
            ElevatorEvent::CallButton { floor, call } => self.handle_call_button(floor, call),
            ElevatorEvent::FloorSensor { floor } => self.handle_floor_sensor_event(floor),
            ElevatorEvent::Obstruction { obstructed } => self.handle_obstruction(obstructed),
            ElevatorEvent::StopButton { .. } => self.state.cabin // Do nothing
        }
    }

    pub fn handle_closed_door_event(&mut self) -> CabinState {
        debug_assert!(self.state.cabin.is_door_open());
        // turn of light and go to idle
        self.elevator.door_light(false);
        self.state.cabin = CabinState::Idle { current_floor: self.state.cabin.get_last_seen_floor() };
        self.state.cabin
    }

    fn handle_call_button(&mut self, floor: u8, call: CallType) -> CabinState {
        // When button pressed, set the light and mark the floor as called
        if let CallType::Cab = call {
            self.elevator.call_button_light(floor, CallType::Cab, true);
            self.state.cab_called[floor as usize] = true;
        }

        self.state.cabin
    }

    fn handle_floor_sensor_event(&mut self, floor: FloorEvent) -> CabinState {
        match floor {
            BetweenFloors() => {
                // Check for initialization state
                if self.state.cabin == CabinState::Init {
                    self.set_motor_direction(MotorDirection::Down);
                }
                self.state.cabin
            }
            AtFloor(floor) => {
                self.elevator.floor_indicator(floor);
                match self.state.target {
                    None => {
                        debug_assert!(self.state.cabin == CabinState::Init);
                        self.reach_idle(floor)
                    },
                    Some(target_floor) => {
                        if target_floor == floor { self.reach_target() }
                        else { self.reach_non_target_floor() }
                    }
                }
            }
        }
    }

    fn handle_obstruction(&mut self, obstructed: bool) -> CabinState {
        self.door_control.update_obstruction(obstructed);
        self.state.cabin
    }

    fn set_motor_direction(&mut self, motor_direction: MotorDirection) {
        if(self.state.cabin.get_direction() != motor_direction) {
            self.state.last_direction = self.state.cabin.get_direction();
        }
        self.elevator.motor_direction(motor_direction);
    }
}