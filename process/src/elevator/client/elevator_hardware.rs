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
    pub fn set_new_target(&mut self, target: u8) -> CabinState {
        match self.state.cabin {
            CabinState::DoorOpen { .. } => {
                println!("Go to floor cannot be called while doors are opened");
                return self.state.cabin
                // unreachable!("Go to floor cannot be called while doors are opened")
            },
            CabinState::Idle { .. }
            | CabinState::Between { .. } => {
                self.state.target = Some(target);

                let from_floor = self.state.cabin.get_current_floor_relative_to(target);
                let direction = self.state.cabin.get_direction_relative_to(target);

                let to_floor = match direction {
                    MotorDirection::Stop => return self.reach_target(),
                    MotorDirection::Down => from_floor - 1,
                    MotorDirection::Up => from_floor + 1
                };

                self.state.last_direction = self.state.cabin.get_direction();
                self.state.cabin = CabinState::Between { from_floor, to_floor };
                self.elevator.motor_direction(direction);
                self.state.cabin
            }
            CabinState::Init => unreachable!("Function go_to_floor cannot be called while elevator is initializing") //TODO
        }
    }

    pub fn offline_handle_next_cab_call(&mut self) {
        // Change call only when idling or if on the way to target 
        match self.state.cabin {
            CabinState::DoorOpen { .. } => return,
            CabinState::Idle { current_floor } => self.find_closest_call(current_floor, None),
            CabinState::Between { from_floor, to_floor } => self.find_closest_call(from_floor, Some(to_floor)),
            CabinState::Init => return,
        }
    }

    pub fn find_closest_call(&mut self, last_floor: u8, to_floor: Option<u8>) {
        let motor_direction = self.state.cabin.get_direction();
        println!("Last floor: {}", last_floor);
        println!("Last direction:{:?}", self.state.last_direction);

        let next_call: Option<(u8, u8)> = self.state
            .cab_called
            .iter()
            .enumerate()
            .filter_map(|(index, is_called)| {
                let target_floor = index as u8;
                let target_direction = self.state.cabin.get_direction_relative_to(target_floor);
                if *is_called &&
                    ((motor_direction == MotorDirection::Up && target_floor > last_floor) ||
                    (motor_direction == MotorDirection::Down && target_floor < last_floor) ||
                    (motor_direction == MotorDirection::Stop ))
                {
                    Some((target_floor, u8::abs_diff(last_floor, target_floor)))
                } else {
                    None
                }

            })
            .min_by(|(_, d1), (_, d2)| {d1.cmp(d2)});

            

        // let next_call: Option<(u8, u8)> = self.state
        //     .cab_called
        //     .iter()
        //     .enumerate()
        //     .filter_map(|(index, is_called)| {
        //         let target_floor = index as u8;
        //         if *is_called{
        //             match motor_direction {
        //                 MotorDirection::Up if target_floor > last_floor => {
        //                     Some((target_floor, u8::abs_diff(last_floor, target_floor)))
        //                 },
        //                 MotorDirection::Down if target_floor < last_floor => {
        //                     Some((target_floor, u8::abs_diff(last_floor, target_floor)))
        //                 },
        //                 MotorDirection::Stop => {
        //                     match last_motordirection => {
        //                         MotorDirection::Up if target_floor > last_floor => {
        //                             Some((target_floor, u8::abs_diff(last_floor, target_floor)))
        //                         },
        //                         MotorDirection::Down if target_floor < last_floor => {
        //                             Some((target_floor, u8::abs_diff(last_floor, target_floor)))
        //                         },
        //                         _ => None //OBS: crash????
        //                     }
        //                 }
        //             }
        //         } else {
        //             None
        //         }

        //     })
        //     .min_by(|(_, d1), (_, d2)| {d1.cmp(d2)});
            
        println!("NEXT_CALL: {next_call:?}");

        if let Some((new_target, _)) = next_call {
            self.set_new_target(new_target);
        } else {
            self.state.last_direction = self.state.cabin.get_direction();
            self.elevator.motor_direction(MotorDirection::Stop);
        }
    }

    pub fn init_if_is_not_yet(&mut self) {
        if self.state.cabin == CabinState::Init {
            self.state.last_direction = self.state.cabin.get_direction();
            self.elevator.motor_direction(MotorDirection::Down);
        }
    }

    pub fn set_call_light_state(&mut self, call_request: CallRequest, on: bool) {
        let floor = call_request.target();
        self.elevator.call_button_light(floor, call_request.into(), on);
    }

    pub fn set_all_call_lights_state(&mut self, on: bool) {
        for floor in 0..N_FLOOR {
            self.elevator.call_button_light(floor, CallType::Cab, on);
            self.elevator.call_button_light(floor, CallType::HallUp, on);
            self.elevator.call_button_light(floor, CallType::HallDown, on);
        }
    }

    pub fn set_hall_lights_state(&mut self, on: bool) {
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
}

impl ElevatorHardwareState {
    fn reach_target(&mut self) -> CabinState {
        self.state.last_direction = self.state.cabin.get_direction();
        self.elevator.motor_direction(MotorDirection::Stop);
        self.elevator.door_light(true);
        self.door_control.open_door();
        let floor_reached = self.state.target.unwrap();
        self.elevator.call_button_light(floor_reached, CallType::Cab, false);
        self.state.cab_called[floor_reached as usize] = false;
        self.state.cabin = CabinState::DoorOpen { current_floor: floor_reached };
        self.state.target = None;
        self.state.cabin
    }

    fn reach_idle(&mut self, floor: u8) -> CabinState {
        self.state.last_direction = self.state.cabin.get_direction();
        self.elevator.motor_direction(MotorDirection::Stop);
        self.state.target = None;
        self.state.cabin = CabinState::Idle { current_floor: floor };
        self.state.cabin
    }

    fn reach_non_target_floor(&mut self) -> CabinState {
        self.state.cabin.increment_between();
        self.state.cabin
    }
}

impl ElevatorHardwareState {
    pub fn handle_native_event(&mut self, event: ElevatorEvent) -> CabinState {
        match event {
            ElevatorEvent::CallButton { floor, call } => self.handle_call_button(floor, call),
            ElevatorEvent::FloorSensor { floor } => self.handle_floor_sensor_event(floor),
            ElevatorEvent::Obstruction { obstructed } => self.handle_obstruction(obstructed),
            ElevatorEvent::StopButton { .. } => self.state.cabin // TODO IMPLEMENT
        }
    }

    pub fn handle_closed_door_event(&mut self) -> CabinState {
        debug_assert!(self.state.cabin.is_door_open());
        self.elevator.door_light(false);
        self.state.cabin = CabinState::Idle { current_floor: self.state.cabin.get_last_seen_floor() };
        self.state.cabin
    }

    fn handle_call_button(&mut self, floor: u8, call: CallType) -> CabinState {
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
                    self.state.last_direction = self.state.cabin.get_direction();
                    self.elevator.motor_direction(MotorDirection::Down);
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
}