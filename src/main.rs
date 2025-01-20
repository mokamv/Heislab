use std::cmp::Ordering;
use std::thread::*;
use std::time::*;

use crossbeam_channel as cbc;

use driver_rust::elevio;
use driver_rust::elevio::elev as e;

struct ElevatorState {
    dirn: u8,
    target: u8,
    from: u8,
    to: u8,

    halt: bool
}

impl ElevatorState {
    fn new() -> ElevatorState {
        ElevatorState {
            dirn: e::DIRN_DOWN,
            from: u8::MAX,
            to: u8::MAX,
            target: 0,
            halt: false
        }
    }

    fn dirn(&self) -> u8 {
        if self.halt {
            e::DIRN_STOP
        } else {
            self.dirn
        }
    }

    fn halt(&mut self, halt: bool) {
        self.halt = halt;
    }

    fn compute_direction(&mut self) {
        self.dirn = match self.target.cmp(&self.to) {
            Ordering::Less => {
                e::DIRN_DOWN
            }
            Ordering::Equal => {
                if self.dirn == e::DIRN_STOP {
                    match self.to.cmp(&self.from) {
                        Ordering::Less => { e::DIRN_UP }
                        Ordering::Equal => { e::DIRN_STOP }
                        Ordering::Greater => { e::DIRN_DOWN }
                    }
                } else {
                    self.dirn
                }
            }
            Ordering::Greater => {
                e::DIRN_UP
            }
        }
    }

    fn update_target(&mut self, target: u8) {
        self.target = target;
        self.compute_direction();
    }

    fn reached_floor(&mut self, floor: u8) -> bool {
        self.from = floor;

        if floor == self.target {
            self.dirn = e::DIRN_STOP;
            self.to = floor;
            true
        } else {
            self.to = floor.wrapping_add(
                match self.dirn {
                    e::DIRN_UP => 1,
                    e::DIRN_DOWN => u8::MAX,
                    _ => 0,
                }
            );
            false
        }
    }
}

fn main() -> std::io::Result<()> {
    let elev_num_floors = 4;
    let elevator = e::Elevator::init("localhost:15657", elev_num_floors)?;
    println!("Elevator started:\n{:#?}", elevator);

    let poll_period = Duration::from_millis(25);

    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();
    {
        let elevator = elevator.clone();
        spawn(move || elevio::poll::call_buttons(elevator, call_button_tx, poll_period));
    }

    let (floor_sensor_tx, floor_sensor_rx) = cbc::unbounded::<u8>();
    {
        let elevator = elevator.clone();
        spawn(move || elevio::poll::floor_sensor(elevator, floor_sensor_tx, poll_period));
    }

    let (stop_button_tx, stop_button_rx) = cbc::unbounded::<bool>();
    {
        let elevator = elevator.clone();
        spawn(move || elevio::poll::stop_button(elevator, stop_button_tx, poll_period));
    }

    let (obstruction_tx, obstruction_rx) = cbc::unbounded::<bool>();
    {
        let elevator = elevator.clone();
        spawn(move || elevio::poll::obstruction(elevator, obstruction_tx, poll_period));
    }

    let mut state = ElevatorState::new();

    match elevator.floor_sensor() {
        None => {
            elevator.motor_direction(state.dirn());
        }
        Some(floor) => {
            state.update_target(floor);
            state.reached_floor(floor);
        }
    }

    loop {
        cbc::select! {
            recv(call_button_rx) -> a => {
                let call_button = a.unwrap();
                println!("{:#?}", call_button);
                elevator.call_button_light(call_button.floor, call_button.call, true);

                state.update_target(call_button.floor);
                elevator.motor_direction(state.dirn());
            },
            recv(floor_sensor_rx) -> a => {
                let floor = a.unwrap();
                println!("Floor: {:#?}", floor);
                let is_target = state.reached_floor(floor);
                if is_target {
                    for c in 0..3 {
                        elevator.call_button_light(floor, c, false);
                    }
                }
                elevator.motor_direction(state.dirn());
            },
            recv(stop_button_rx) -> a => {
                let stop = a.unwrap();
                println!("Stop button: {:#?}", stop);
                for f in 0..elev_num_floors {
                    for c in 0..3 {
                        elevator.call_button_light(f, c, false);
                    }
                }
            },
            recv(obstruction_rx) -> a => {
                let obstr = a.unwrap();
                println!("Obstruction: {:#?}", obstr);
                state.halt(obstr);
                elevator.motor_direction(state.dirn());
            },
        }
    }
}