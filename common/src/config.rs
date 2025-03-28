use std::time::Duration;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;

/// Expected number of floors
pub const N_FLOOR: u8 = 4;

/// Expected number of clients
pub const CLIENT_COUNT: usize = 3;

/// Possible values for clients ids.
pub const VALID_CLIENT_IDS: [ConnectionIdentifier; CLIENT_COUNT as usize] =
    [0,1,2];

/// Rate at which the Elevator Hardware should be polled for events.
pub const HW_POLL_PERIOD: Duration = Duration::from_millis(25);

/// Address of the Elevator Hardware (or simulator)
pub const HW_ADDRESS: &str = "127.0.0.1:15001";

/// Period at which the emergency light should blink while disconnected from the controller
pub const EMERGENCY_BLINKING_PERIOD: Duration = Duration::from_millis(1000);

/// Delay before a backup node can promote itself to master in the situation that the other controller
/// isn't responding or signaling its presence.
pub const DELAY_TO_BECOME_MASTER: Duration = Duration::from_secs(1);

/// For how long the doors stay open
pub const DOOR_OPEN_DURATION: Duration = Duration::from_secs(3);


/// Network related config

/// Port at which the controller bind to send its broadcast frames
pub const CONTROLLER_BC_BIND_PORT: u16 = 9000;
/// Port used to send and receive the broadcast frames.
pub const CONTROLLER_BC_PORT: u16 = 9001;
