use std::time::Duration;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;

pub const N_FLOOR: u8 = 4;

pub const CLIENT_COUNT: u8 = 3;

pub const VALID_CLIENT_IDS: [ConnectionIdentifier; CLIENT_COUNT as usize] =
    [0,1,2];

pub const HW_POLL_PERIOD: Duration = Duration::from_millis(25); // TODO MOVE TO A CONFIG FILE

pub const HW_ADDRESS: &str = "127.0.0.1:15657";

pub const EMERGENCY_BLINKING_PERIOD: Duration = Duration::from_millis(1000);