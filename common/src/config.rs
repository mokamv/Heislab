use std::time::Duration;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;

pub const N_FLOOR: u8 = 4;

pub const CLIENT_COUNT: u8 = 3;

pub const VALID_CLIENT_IDS: [ConnectionIdentifier; CLIENT_COUNT as usize] =
    [0,1,2];

pub const HW_POLL_PERIOD: Duration = Duration::from_millis(25);

pub const HW_ADDRESS: &str = "127.0.0.1:15000";

pub const EMERGENCY_BLINKING_PERIOD: Duration = Duration::from_millis(1000); //TODO MIGHT MOVE TO CONST SINCE TOO LOW AND IT BREAKS

pub const DELAY_TO_BECOME_MASTER: Duration = Duration::from_secs(3);

pub const DOOR_OPEN_DURATION: Duration = Duration::from_secs(3);


/// Network related config

/// Port at which the controller bind to send its broadcast frames
pub const CONTROLLER_BC_BIND_PORT: u16 = 9000;
/// Port used to send and receive the broadcast frames.
pub const CONTROLLER_BC_PORT: u16 = 9001;
