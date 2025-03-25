use crate::connection::receiver::wrapper::udp_broadcast_receiver::UdpBroadcastReceiver;
use crate::connection::receiver::wrapper::udp_receiver::UdpDataReceiver;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};

pub(super) enum Receiver {
    UDPDataSocket(UdpDataReceiver),
    UDPBroadcastSocket(UdpBroadcastReceiver),
}

impl IntoRawFd for Receiver {
    fn into_raw_fd(self) -> RawFd {
        match self {
            Receiver::UDPBroadcastSocket(receiver) => receiver.into_raw_fd(),
            Receiver::UDPDataSocket(receiver) => receiver.into_raw_fd(),
        }
    }
}

impl AsRawFd for Receiver {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Receiver::UDPBroadcastSocket(receiver) => receiver.as_raw_fd(),
            Receiver::UDPDataSocket(receiver) => receiver.as_raw_fd(),
        }
    }
}