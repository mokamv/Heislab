use driver_rust::elevio::elev::Elevator;
use crate::elevator::client::door_control::DoorControl;
use crate::elevator::client::elevator_event::EventChannel;

pub struct ElevatorInteraction {
    pub event_channel: EventChannel,
    door_control: DoorControl,
}

impl ElevatorInteraction {
    pub fn new(addr: &str, floor_count: u8) -> Self {
        let (door_control, close_door_rx) = DoorControl::new();
        let elevator = Elevator::init(addr, floor_count).expect("TODO");

        Self {
            door_control,
            event_channel: EventChannel::new(elevator, close_door_rx)
        }
    }

    pub fn go_to_floor(&self) {

    }

    pub fn obstructed(&self, is_obstructed: bool) {
        self.door_control.obstruction(is_obstructed);
    }

    pub fn open_door(&self) {
        self.door_control.open();
    }
}