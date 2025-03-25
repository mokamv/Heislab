use std::net::{SocketAddr};
use std::time::Instant;
pub type ConnectionIdentifier = u8;

pub const HANDLE_ACK_UNINIT: usize = usize::MAX;
pub const HANDLE_HASH_UNINIT: usize = usize::MAX;

#[derive(Debug, Copy, Clone)]
pub enum HandleState {
    /// A handle is considered [Connected](Pending) when it contains an active and alive
    /// [TcpStream].
    Connected {
        /// For an elevator client, this value is the id of the current controller, it is used for reconciliation purpose.
        /// For a backup node, it allows controller_link with the master
        /// For the elevator controller, the value is the id of the elevator the handle refers to.
        ///
        /// A client seeing two master controllers will always pick the one of lowest id.
        connected_to: ConnectionIdentifier,
        address: SocketAddr,
        last_update: Instant,
        since: Instant,
        ack: usize,
        hash: usize
    },
    /// A handle is considered [Disconnected](Disconnected) when it does not contain an active and alive
    /// [UdpSocket], but connection is still possible later in time.
    Disconnected {
        since: Instant
    },
}

impl Default for HandleState {
    fn default() -> Self {
        Self::Disconnected {
            since: Instant::now(),
        }
    }
}