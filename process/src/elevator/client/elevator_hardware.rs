use common::data_struct::{CabinState, CallRequest};
use crate::elevator::client::door_control::DoorControl;
use driver_rust::elevio::elev::{Elevator, ElevatorEvent, MotorDirection};
use std::time::Duration;
use crossbeam_channel::Receiver;

pub struct MinimalState {
    cabin: CabinState,
    target: Option<u8>,
}

pub struct ElevatorHardware {
    event_receiver: Option<Receiver<ElevatorEvent>>,
    close_door_receiver: Option<Receiver<()>>,
    state: MinimalState,
    elevator: Elevator,
    door_control: DoorControl,
}

impl ElevatorHardware {
    pub fn new(addr: &str, floor_count: u8, poll_period: Duration) -> Self {
        let (door_control, close_door_rx) = DoorControl::new(poll_period);
        let mut elevator = Elevator::init(addr, floor_count).expect("TODO");
        elevator.event_loop(poll_period);

        Self {
            door_control,
            state: MinimalState {
                target: None,
                cabin: Default::default(),
            },
            event_receiver: Some(elevator.event_receiver.clone()),
            close_door_receiver: Some(close_door_rx),
            elevator,
        }
    }

    pub fn take_receivers(&mut self) -> (Receiver<ElevatorEvent>, Receiver<()>) {
        (
            self.event_receiver.take().expect("You can only take events receivers once"),
            self.close_door_receiver.take().expect("You can only take events receivers once")
        )
    }
}

impl ElevatorHardware {
    pub fn go_to_floor(&mut self, target: u8) -> CabinState {
        match self.state.cabin {
            CabinState::DoorOpen { .. } => {
                debug_assert!(false);
                unreachable!("Go to floor cannot be called while doors are opened")
            }
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

                self.state.cabin = CabinState::Between { from_floor, to_floor };
                self.elevator.motor_direction(direction);
                self.state.cabin
            }
            CabinState::Init => unreachable!("Function go_to_floor cannot be called while elevator is initializing") //TODO
        }
    }

    pub fn init_if_is_not_yet(&mut self) {
        if self.state.target == None
            && self.state.cabin == Default::default() {
            self.elevator.motor_direction(MotorDirection::Down)
        }
    }

    pub fn call_button_light(&mut self, call_request: CallRequest, on: bool) {
        let floor = call_request.target();
        self.elevator.call_button_light(floor, call_request.into(), on);
    }
}

impl ElevatorHardware {
    fn reach_target(&mut self) -> CabinState {
        self.elevator.motor_direction(MotorDirection::Stop);
        self.elevator.door_light(true);
        self.door_control.open_door();
        self.state.cabin = CabinState::DoorOpen { current_floor: self.state.target.unwrap() };
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

impl ElevatorHardware {
    pub fn handle_event(&mut self, event: ElevatorEvent) -> CabinState {
        match event {
            ElevatorEvent::CallButton { .. } => self.state.cabin, // Do nothing
            ElevatorEvent::FloorSensor { floor } => self.handle_floor_sensor(floor),
            ElevatorEvent::Obstruction { obstructed } => self.handle_obstruction(obstructed),
            ElevatorEvent::StopButton { .. } => self.state.cabin // TODO IMPLEMENT
        }
    }

    pub fn handle_close_door(&mut self) -> CabinState {
        debug_assert!(self.state.cabin.is_door_open());
        self.elevator.door_light(false);
        self.state.cabin = CabinState::Idle { current_floor: self.state.cabin.get_last_seen_floor() };
        self.state.cabin
    }

    fn handle_floor_sensor(&mut self, floor: u8) -> CabinState {
        self.elevator.floor_indicator(floor);
        match self.state.target {
            None => self.reach_idle(floor),
            Some(target_floor) => {
                if target_floor == floor { self.reach_target() }
                else { self.reach_non_target_floor() }
            }
        }
    }

    fn handle_obstruction(&mut self, obstructed: bool) -> CabinState {
        self.door_control.obstruction(obstructed);
        self.state.cabin
    }
}