use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use crate::connection::constants::ip_addresses::CONTROLLER_UDP_BIND_PORT;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::connection::receiver::wrapper::udp_broadcast_receiver::ErrorKind::{RetryError, UnexpectedMessageType};
use crate::connection::udp_impl::shared_udp_socket::udp_socket_sharing_port;
use crate::connection::udp_impl::udp_read::{upd_read_one_bc_message_on_port, UdpReadError};
use crate::data_struct::ControllerState;
use crate::messages::Message;

pub(in super::super) struct UdpBroadcastReceiver {
    udp_socket: UdpSocket
}

impl AsRawFd for UdpBroadcastReceiver {
    fn as_raw_fd(&self) -> RawFd {
        self.udp_socket.as_raw_fd()
    }
}

impl IntoRawFd for UdpBroadcastReceiver {
    fn into_raw_fd(self) -> RawFd {
        self.udp_socket.into_raw_fd()
    }
}

impl UdpBroadcastReceiver {
    pub(in super::super) fn bind<A: ToSocketAddrs>(bind_to: A) -> Self {
        let udp_socket = udp_socket_sharing_port(bind_to).unwrap();
        let _ = udp_socket.set_nonblocking(true).unwrap();

        Self {
            udp_socket,
        }
    }

    pub(in super::super) fn handle_broadcast(
        &mut self
    ) -> Result<(ConnectionIdentifier, ControllerState, SocketAddr), UdpBroadcastError> {
        let message = upd_read_one_bc_message_on_port(
            &mut self.udp_socket,
            CONTROLLER_UDP_BIND_PORT
        )?;
        match message {
            Message::ControllerAddress {
                id,
                state,
                address
            } => Ok((id, state, address)),

            _ => Err(UdpBroadcastError::new(UnexpectedMessageType))
        }
    }
}



#[derive(Debug)]
pub struct UdpBroadcastError {
    kind: ErrorKind
}

impl From<UdpReadError> for UdpBroadcastError {
    fn from(value: UdpReadError) -> Self {
        let kind = match value {
            UdpReadError::SocketError
            | UdpReadError::InvalidMessageSize
            | UdpReadError::InvalidMessage => UnexpectedMessageType,
            UdpReadError::NonControllerFrame
            | UdpReadError::ReadBlocked => RetryError
        };

        Self::new(kind)
    }
}

impl UdpBroadcastError {
    pub fn new(error_kind: ErrorKind) -> Self {
        Self {
            kind: error_kind
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ErrorKind {
    UnexpectedMessageType,
    RetryError
}