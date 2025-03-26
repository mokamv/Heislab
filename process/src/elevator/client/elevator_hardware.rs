use common::data_struct::{CabinState, CallRequest};
use crate::elevator::client::door_control::DoorControl;
use driver_rust::elevio::elev::{Elevator, ElevatorEvent, FloorEvent, MotorDirection};
use std::time::Duration;
use crossbeam_channel::Receiver;
use driver_rust::elevio::elev::FloorEvent::{AtFloor, BetweenFloors};

pub struct HardwareState {
    cabin: CabinState,
    target: Option<u8>,
}

pub struct ElevatorHardwareState {
    state: HardwareState,
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
            state: HardwareState {
                target: None,
                cabin: Default::default(),
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
            CabinState::OpenedDoor { .. } => unreachable!("Go to floor cannot be called while doors are opened"),
            CabinState::Idle { .. }
            | CabinState::BetweenFloors { .. } => {
                self.state.target = Some(target);

                let from_floor = self.state.cabin.get_current_floor_relative_to(target);
                let direction = self.state.cabin.get_direction_relative_to(target);

                let to_floor = match direction {
                    MotorDirection::Stop => return self.reach_target(),
                    MotorDirection::Down => from_floor - 1,
                    MotorDirection::Up => from_floor + 1
                };

                self.state.cabin = CabinState::BetweenFloors { from_floor, to_floor };
                self.elevator.motor_direction(direction);
                self.state.cabin
            }
        }
    }

    pub fn set_call_light_state(&mut self, call_request: CallRequest, on: bool) {
        let floor = call_request.target();
        self.elevator.call_button_light(floor, call_request.into(), on);
    }

    pub fn get_current_cabin_state(&self) -> CabinState {
        self.state.cabin
    }
}

impl ElevatorHardwareState {
    fn reach_target(&mut self) -> CabinState {
        self.elevator.motor_direction(MotorDirection::Stop);
        self.elevator.door_light(true);
        self.door_control.open_door();
        self.state.cabin = CabinState::OpenedDoor { current_floor: self.state.target.unwrap() };
        self.state.target = None;
        self.state.cabin
    }

    fn reach_idle(&mut self, floor: u8) -> CabinState {
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
            ElevatorEvent::CallButton { .. } => self.state.cabin, // Do nothing
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

    fn handle_floor_sensor_event(&mut self, floor: FloorEvent) -> CabinState {
        match floor {
            BetweenFloors() => {
                // No target + in between floor -> Repositioning
                if self.state.target == None {
                    self.elevator.motor_direction(MotorDirection::Down);
                }
                self.state.cabin
            }
            AtFloor(floor) => {
                self.elevator.floor_indicator(floor);
                match self.state.target {
                    None => self.reach_idle(floor),
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