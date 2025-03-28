use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use crate::config::{CONTROLLER_BC_BIND_PORT, CONTROLLER_BC_PORT};

/// Button indices for elevator request matrix

/// Index used to store the hall_up request value inside requests matrices
pub const HALL_UP_IDX: usize = 0;
/// Index used to store the hall_down request value inside requests matrices
pub const HALL_DOWN_IDX: usize = 1;
/// Index used to store the cab request value inside requests matrices
pub const CAB_IDX: usize = 2;
pub const N_HALL_BUTTONS: usize = 2;
/// Number of buttons for a specific floor (does not consider edge cases)
pub const N_BUTTONS: usize = 3;


/// Network addresses related constants
pub const GLOBAL_PORT: u16 = 0;
const BROADCAST_ADDRESS: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 255);
const GLOBAL_ADDRESS: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
/// Every socket that job is to listen to the controller broadcast should bind to this address.
pub const UDP_BC_LISTEN_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(GLOBAL_ADDRESS), CONTROLLER_BC_PORT);
/// The controller should bind to this address.
pub const CONTROLLER_BC_BIND_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(GLOBAL_ADDRESS), CONTROLLER_BC_BIND_PORT);
/// The controller should send broadcast frame with this address.
pub const CONTROLLER_BC_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(BROADCAST_ADDRESS), CONTROLLER_BC_PORT);




/// Network inner working related constants

/// Period at which a controller is sending a broadcast message to indicate its own ip address and state.
/// Lower values tends to make clients and other controllers aware of the sender quicker.
pub const BROADCAST_PERIOD: Duration = Duration::from_millis(250);


/// Equivalent to an RTO in TCP, resend packet after this delay if no acknowledgement has been received.
pub const UDP_RESEND_AFTER: Duration = Duration::from_millis(5);
/// Timeout after which a UDP socket is dropped and the state of the handle goes to Disconnected.
/// Higher values tends to make the socket more reliable during high packet losses
/// Lower values tends to make the socket notices dead peer faster (great when no packet loss)
pub const UDP_TIMEOUT: Duration = Duration::from_millis(750);
/// Period at which a keep alive payload is sent to the other peer,
/// Lower values tends to make the socket more resilient to high packet losses
/// Higher values can be used to allow lower values of [UDP_TIMEOUT] without affecting the resiliency to packet loss.
/// Higher values might be more demanding on the CPU (TODO not tested yet).
pub const UDP_KEEP_ALIVE_PERIOD: Duration = Duration::from_millis(25);
