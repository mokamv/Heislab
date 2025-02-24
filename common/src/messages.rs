use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use crate::messages::Message::{ClientAuth, KeepAlive, SocketAddress};

pub type RawMessage = [u8; MESSAGE_SIZE];
pub const MESSAGE_SIZE: usize = 32;
pub const DEFAULT_MESSAGE: [u8; MESSAGE_SIZE] = [0u8; MESSAGE_SIZE];
pub use DEFAULT_MESSAGE as KEEP_ALIVE_MESSAGE;


// Limited to 255 types of messages if coding message type on the first byte
#[derive(Debug)]
pub enum Message {
    KeepAlive,
    SocketAddress{ address: SocketAddr },
    ClientAuth{ client_id: u8 }, // Client identifier
}

impl Message {

    pub fn is_keep_alive(raw_message: &RawMessage) -> bool {
        raw_message[0] == 0u8
    }

    pub fn encode(self) -> RawMessage {
        let mut raw_message = [0u8; MESSAGE_SIZE];
        match self {
            KeepAlive => {}
            SocketAddress { address } => {
                raw_message[0] = 1;
                match address.ip() {
                    IpAddr::V4(ipv4) => {
                        raw_message[1] = 4;
                        raw_message[2..6].copy_from_slice(&ipv4.octets());
                        raw_message[6..6 + size_of::<u16>()].copy_from_slice(&address.port().to_be_bytes())
                    }
                    IpAddr::V6(ipv6) => {
                        raw_message[1] = 6;
                        raw_message[2..18].copy_from_slice(&ipv6.octets());
                        raw_message[18..18 + size_of::<u16>()].copy_from_slice(&address.port().to_be_bytes())
                    }
                }
            }
            ClientAuth { client_id } => {
                raw_message[0] = 2;
                raw_message[1] = client_id
            }
        }

        raw_message
    }
    pub fn decode_message(raw_message: &RawMessage) -> Self {
        match raw_message[0] {
            0 => KeepAlive,
            1 => SocketAddress { address: match raw_message[1] {
                    4 => {
                        let mut ip = [0u8; 4]; let mut port = [0u8; 2];
                        ip.copy_from_slice(&raw_message[2..6]);
                        port.copy_from_slice(&raw_message[6..8]);
                        SocketAddr::from(SocketAddrV4::new(Ipv4Addr::from(ip), u16::from_be_bytes(port)))
                    },
                    6 => {
                        let mut ip = [0u8; 16]; let mut port = [0u8; size_of::<u16>()];
                        ip.copy_from_slice(&raw_message[2..18]);
                        port.copy_from_slice(&raw_message[18..size_of::<u16>()]);
                        SocketAddr::from(SocketAddrV6::new(Ipv6Addr::from(ip), u16::from_be_bytes(port), 0, 0))
                    }
                    _ => panic!("TODO")
                }, },
            2 => ClientAuth { client_id: raw_message[1] },
            _ => panic!("TODO"),
        }
    }
}