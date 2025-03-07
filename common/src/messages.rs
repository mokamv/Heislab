use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Instant;
use driver_rust::elevio::elev::{CallType, ElevatorEvent, MotorDirection};
use Message::{ClientStopButton, Connected, ControllerAuth, Disconnected};
use crate::connection::connection_handle::channel::{AliveStatus, AliveValue};
use crate::messages::Message::{Authenticated, ClientAuth, ClientButtonCall, ClientObstructed, ClientCabinState, ControllerAddress, ControllerCurrentState, GotoFloor, KeepAlive, LightControl};

type RawMessage = [u8; MESSAGE_SIZE];
pub const MESSAGE_SIZE: usize = 32;
pub const DEFAULT_MESSAGE: [u8; MESSAGE_SIZE] = [0u8; MESSAGE_SIZE];
use crate::connection::controller_state::ControllerState;
use crate::data_struct::{CabinState, CallRequest};
use crate::data_struct::CallRequest::{Cab, Hall};

#[derive(Debug)]
pub struct TimedMessage {
    timestamp: Instant,
    message: Message
}

impl TimedMessage {
    pub fn of(message: Message) -> Self {
        Self {
            timestamp: Instant::now(),
            message,
        }
    }

    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    pub fn message(self) -> Message {
        self.message
    }
}

// Limited to 255 types of messages if coding message type on the first byte
#[derive(Debug, Copy, Clone)]
pub enum Message {
    KeepAlive,

    // Client messages
    ClientObstructed { is_obstructed: bool },
    ClientCabinState { cabin_state: CabinState },
    ClientButtonCall { pressed: CallRequest },
    ClientStopButton { is_pressed: bool },

    // Controller messages
    ControllerAddress { id: u8, state: ControllerState, address: SocketAddr },
    LightControl { button: CallRequest, is_lit: bool },
    GotoFloor { go_to_floor: u8 },

    // Connection State flow
    Connected,
    Authenticated,
    Disconnected,
    ClientAuth{ client_id: u8 }, // Client identifier

    // Synchronisation messages
    ControllerAuth { controller_id: u8 },
    ControllerCurrentState { id: u8, state: ControllerState },
}

impl Message {

    pub fn is_keep_alive(&self) -> bool {
        if let KeepAlive = self {
            true
        } else { false }
    }

    pub fn encode(self) -> RawMessage {
        let mut raw_message = [0u8; MESSAGE_SIZE];
        match self {
            KeepAlive => {}

            // Client encode
            ClientObstructed { is_obstructed } => {
                raw_message[0] = 1;
                raw_message[1] = is_obstructed as u8

            },
            ClientCabinState { cabin_state } => {
                raw_message[0] = 2;
                raw_message[1..4].copy_from_slice(&cabin_state.encode());
            },
            ClientButtonCall { pressed } => {
                raw_message[0] = 3;
                raw_message[1..4].copy_from_slice(&pressed.encode());
            },
            ClientStopButton { is_pressed } => {
                raw_message[0] = 4;
                raw_message[1] = is_pressed as u8;
            }

            // Controller encode
            ControllerAddress { id, state, address } => {
                raw_message[0] = 128;
                raw_message[1] = id;
                raw_message[2] = state.into();
                match address.ip() {
                    IpAddr::V4(ipv4) => {
                        raw_message[3] = 4;
                        raw_message[4..8].copy_from_slice(&ipv4.octets());
                        raw_message[8..8 + size_of::<u16>()].copy_from_slice(&address.port().to_be_bytes())
                    }
                    IpAddr::V6(ipv6) => {
                        raw_message[3] = 6;
                        raw_message[4..20].copy_from_slice(&ipv6.octets());
                        raw_message[20..20 + size_of::<u16>()].copy_from_slice(&address.port().to_be_bytes())
                    }
                }
            },
            LightControl { button: target, is_lit } => {
                raw_message[0] = 129;
                raw_message[1..4].copy_from_slice(&target.encode());
                raw_message[4] = is_lit as u8;
            },
            GotoFloor { go_to_floor } => {
                raw_message[0] = 130;
                raw_message[1] = go_to_floor;
            },

            // State flow
            Connected => raw_message[0] = 160,
            Disconnected => raw_message[0] = 161,
            Authenticated => raw_message[0] = 162,
            ClientAuth { client_id } => {
                raw_message[0] = 163;
                raw_message[1] = client_id
            }


            // Synchro
            ControllerAuth { controller_id } => {
                raw_message[0] = 192;
                raw_message[1] = controller_id;
            }

            ControllerCurrentState { id, state } => {
                raw_message[0] = 193;
                raw_message[1] = id;
                raw_message[2] = state.into();
            }
        }

        raw_message
    }
    pub fn decode_message(raw_message: &RawMessage) -> Self {
        match raw_message[0] {
            0 => KeepAlive,

            // Client messages
            1 => ClientObstructed { is_obstructed: raw_message[1] != 0 },
            2 => ClientCabinState { cabin_state: CabinState::decode(&raw_message[1..4]) },
            3 => ClientButtonCall { pressed: CallRequest::decode(&raw_message[1..4]) },
            4 => ClientStopButton { is_pressed: raw_message[1] != 0 },
            
            // Controller messages
            128 => ControllerAddress {
                id: raw_message[1],
                state: raw_message[2].into(),
                address: match raw_message[3] {
                    4 => {
                        let mut ip = [0u8; 4]; let mut port = [0u8; 2];
                        ip.copy_from_slice(&raw_message[4..8]);
                        port.copy_from_slice(&raw_message[8..8 + size_of::<u16>()]);
                        SocketAddr::from(SocketAddrV4::new(Ipv4Addr::from(ip), u16::from_be_bytes(port)))
                    },
                    6 => {
                        let mut ip = [0u8; 16]; let mut port = [0u8; size_of::<u16>()];
                        ip.copy_from_slice(&raw_message[4..20]);
                        port.copy_from_slice(&raw_message[20..20 + size_of::<u16>()]);
                        SocketAddr::from(SocketAddrV6::new(Ipv6Addr::from(ip), u16::from_be_bytes(port), 0, 0))
                    }
                    _ => panic!("TODO")
                } },
            129 => LightControl { button: CallRequest::decode(&raw_message[1..4]), is_lit: raw_message[4] != 0 },
            130 => GotoFloor { go_to_floor: raw_message[1] },


            160 => Connected,
            161 => Disconnected,
            162 => Authenticated,
            163 => ClientAuth { client_id: raw_message[1] },

            192 => ControllerAuth { controller_id: raw_message[1] },
            193 => ControllerCurrentState {
                id: raw_message[1],
                state: raw_message[2].into(),
            },

            code => panic!("Bad message code received: {code}"),
        }
    }
}

impl From<ElevatorEvent> for Message {
    fn from(value: ElevatorEvent) -> Self {
        match value {
            ElevatorEvent::CallButton { floor, call } => ClientButtonCall {
                pressed: match call {
                    CallType::HallUp => Hall { floor, direction: MotorDirection::Up },
                    CallType::HallDown => Hall { floor, direction: MotorDirection::Down },
                    CallType::Cab => Cab { floor }
                }
            },
            ElevatorEvent::FloorSensor { .. } => KeepAlive, // Use keep alive as a non-message.
            ElevatorEvent::Obstruction { obstructed } => ClientObstructed { is_obstructed: obstructed },
            ElevatorEvent::StopButton { stopped } => ClientStopButton { is_pressed: stopped }
        }
    }
}

impl From<AliveStatus> for Message {
    fn from(value: AliveStatus) -> Self {
        match value.value() {
            AliveValue::Connected => Connected,
            AliveValue::Disconnected => Disconnected,
            AliveValue::ConnectedAndAuthenticated => Authenticated
        }
    }
}

impl From<CabinState> for Message {
    fn from(cabin_state: CabinState) -> Self {
        ClientCabinState { cabin_state }
    }
}