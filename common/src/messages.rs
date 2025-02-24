use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use crate::messages::Message::{ClientAuth, ClientButtonCall, ClientObstructed, ClientReachFloor, GotoFloor, KeepAlive, LightControl, SocketAddress};

pub type RawMessage = [u8; MESSAGE_SIZE];
pub const MESSAGE_SIZE: usize = 32;
pub const DEFAULT_MESSAGE: [u8; MESSAGE_SIZE] = [0u8; MESSAGE_SIZE];
pub use DEFAULT_MESSAGE as KEEP_ALIVE_MESSAGE;
use crate::messages::PhysicalButton::{CAB, HALL};

// Limited to 255 types of messages if coding message type on the first byte
#[derive(Debug)]
pub enum Message {
    KeepAlive,

    // Client messages
    ClientAuth{ client_id: u8 }, // Client identifier
    ClientObstructed { is_obstructed: bool },
    ClientReachFloor { floor_reached: u8 },
    ClientButtonCall { pressed: PhysicalButton },

    // Controller messages
    SocketAddress{ address: SocketAddr },
    LightControl { target: PhysicalButton, is_lit: bool },
    GotoFloor { go_to_floor: u8 }
}

impl Message {

    pub fn is_keep_alive(raw_message: &RawMessage) -> bool {
        raw_message[0] == 0u8
    }

    pub fn encode(self) -> RawMessage {
        let mut raw_message = [0u8; MESSAGE_SIZE];
        match self {
            KeepAlive => {}

            // Client encode
            ClientAuth { client_id } => {
                raw_message[0] = 1;
                raw_message[1] = client_id
            }
            ClientObstructed { is_obstructed } => {
                raw_message[0] = 2;
                raw_message[1] = is_obstructed as u8

            },
            ClientReachFloor { floor_reached } => {
                raw_message[0] = 3;
                raw_message[1] = floor_reached
            },
            ClientButtonCall { pressed } => {
                raw_message[0] = 4;
                raw_message[1..4].copy_from_slice(&pressed.encode());
            },

            // Controller encode
            SocketAddress { address } => {
                raw_message[0] = 128;
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
            },
            LightControl { target, is_lit } => {
                raw_message[0] = 129;
                raw_message[1..4].copy_from_slice(&target.encode());
                raw_message[4] = is_lit as u8;
            },
            GotoFloor { go_to_floor } => {
                raw_message[0] = 130;
                raw_message[1] = go_to_floor;
            },
        }

        raw_message
    }
    pub fn decode_message(raw_message: &RawMessage) -> Self {
        match raw_message[0] {
            0 => KeepAlive,
            1 => ClientAuth { client_id: raw_message[1] },
            2 => ClientObstructed { is_obstructed: raw_message[1] != 0 },
            3 => ClientReachFloor { floor_reached: raw_message[1] },
            4 => ClientButtonCall { pressed: PhysicalButton::decode(&raw_message[1..4]) },



            128 => SocketAddress { address: match raw_message[1] {
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
            129 => LightControl { target: PhysicalButton::decode(&raw_message[1..4]), is_lit: raw_message[4] != 0 },
            130 => GotoFloor { go_to_floor: raw_message[1] },
            _ => panic!("TODO"),
        }
    }
}


#[derive(Debug)]
pub enum PhysicalButton {
    HALL { floor: u8, direction_is_up: bool },
    CAB { floor: u8 }
}

impl PhysicalButton {
    fn encode(&self) -> [u8; 3] {
        let mut message = [0u8; 3];
        match self {
            HALL { floor, direction_is_up } => {
                message[0] = 0;
                message[1] = *floor;
                message[2] = *direction_is_up as u8;
            }
            CAB { floor } => {
                message[0] = 1;
                message[1] = *floor;

            }
        };
        message
    }

    fn decode(raw_button: &[u8]) -> Self {
        assert_eq!(raw_button.len(), 3);
        match raw_button[0] {
            0 => HALL { floor: raw_button[1], direction_is_up: raw_button[2] != 0 },
            _ => CAB { floor: raw_button[1] }
        }
    }
}