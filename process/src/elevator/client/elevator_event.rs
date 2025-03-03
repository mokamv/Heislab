use std::thread::spawn;
use std::time::Duration;
use crossbeam_channel::{unbounded, Receiver};
use driver_rust::elevio;
use driver_rust::elevio::elev::Elevator;
use driver_rust::elevio::poll::CallButton;

pub struct EventChannel {
    pub(crate) call_button_rx: Receiver<CallButton>,
    pub(crate) close_door_rx: Receiver<()>,
    pub(crate) floor_sensor_rx: Receiver<u8>,
    pub(crate) stop_button_rx: Receiver<bool>,
    pub(crate) obstruction_rx: Receiver<bool>
}

impl EventChannel {
    pub fn new(elevator: Elevator, close_door_rx: Receiver<()>) -> EventChannel {
        let poll_period = Duration::from_millis(25);

        let (call_button_tx, call_button_rx) = unbounded::<CallButton>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::call_buttons(elevator, call_button_tx, poll_period));
        }

        let (floor_sensor_tx, floor_sensor_rx) = unbounded::<u8>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::floor_sensor(elevator, floor_sensor_tx, poll_period));
        }

        let (stop_button_tx, stop_button_rx) = unbounded::<bool>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::stop_button(elevator, stop_button_tx, poll_period));
        }

        let (obstruction_tx, obstruction_rx) = unbounded::<bool>();
        {
            let elevator = elevator.clone();
            spawn(move || elevio::poll::obstruction(elevator, obstruction_tx, poll_period));
        }

        EventChannel {
            close_door_rx,
            call_button_rx,
            floor_sensor_rx,
            stop_button_rx,
            obstruction_rx,
        }
    }
}