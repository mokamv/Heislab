use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use crate::connection::udp_impl::udp_read::udp_read_one_message;
use crate::data_structures::network::payload::TimedPayload;

pub(in super::super) struct UdpDataReceiver {
    listener: UdpSocket
}

impl IntoRawFd for UdpDataReceiver {
    fn into_raw_fd(self) -> RawFd {
        self.listener.into_raw_fd()
    }
}

impl AsRawFd for UdpDataReceiver {
    fn as_raw_fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }
}

impl UdpDataReceiver {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> Self {
        let listener = UdpSocket::bind(addr).unwrap();
        let _ = listener.set_nonblocking(true).unwrap();

        Self {
            listener,
        }
    }

    pub(in super::super) fn get_sender(&self) -> UdpSocket {
        self.listener.try_clone().unwrap()
    }

    pub(in super::super) fn recv_message(&mut self) -> io::Result<(TimedPayload, SocketAddr)> {
        let message = udp_read_one_message(&mut self.listener);

        match message {
            Ok(message) => Ok(message),
            Err(error) => {
                println!("Failed to receive message: {error:?}");
                Err(error.into())
            }
        }
    }
}