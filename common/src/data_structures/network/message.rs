use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Instant;
use driver_rust::elevio::elev::{CallType, ElevatorEvent, MotorDirection};
use crate::config::N_FLOOR;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::data_structures::cabin_state::CabinState;
use crate::data_structures::call_light_array::{CallLightArray, RAW_CALL_LIGHT_ARRAY_SIZE};
use crate::data_structures::call_request::CallRequest;
use crate::data_structures::call_request::CallRequest::{Cab, Hall};
use crate::data_structures::controller_state::ControllerState;
use crate::data_structures::full_requests_matrix::{FullControllerRequestsMatrix, FCRM_RAW_SIZE};
use crate::data_structures::network::message::Message::{KeepAlive, Ack,Connected, Disconnected, ClientButtonCall, ClientCabinState, ClientObstructed, ClientStopButton, ClientSyncCab, ControllerAddress, ControllerSyncMerge, ControllerSyncReplace, ControllerSyncState, FullCallLightControl, GotoFloor, LightControl, ControllerSyncFinish};

pub const RAW_MESSAGE_SIZE: usize = 32;
pub type RawMessage = [u8; RAW_MESSAGE_SIZE];
pub const UNINIT_RAW_MESSAGE: RawMessage = [0u8; RAW_MESSAGE_SIZE];

#[derive(Debug)]
pub struct TimedMessage {
    pub(in super) timestamp: Instant,
    pub(in super) message: Message
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

    pub fn message(&self) -> Message {
        self.message
    }
}

// Limited to 255 types of messages if coding message type on the first byte
#[derive(Debug, Copy, Clone)]
pub enum Message {
    KeepAlive,
    Ack,

    // Client messages
    ClientObstructed { is_obstructed: bool },
    ClientCabinState { cabin_state: CabinState },
    ClientButtonCall { pressed: CallRequest },
    ClientStopButton { is_pressed: bool },
    ClientSyncCab { cab_pressed: [bool; N_FLOOR as usize] },

    // Controller messages
    ControllerAddress { id: u8, state: ControllerState, address: SocketAddr },
    LightControl { button: CallRequest, is_lit: bool },
    GotoFloor { go_to_floor: u8 },
    FullCallLightControl { light_array: CallLightArray },

    // Connection State flow
    Connected,
    Disconnected,

    // Synchronisation messages
    ControllerSyncState { controller_id: ConnectionIdentifier, controller_state: ControllerState },
    ControllerSyncReplace { full_matrix: FullControllerRequestsMatrix },
    ControllerSyncMerge { full_matrix: FullControllerRequestsMatrix },
    ControllerSyncFinish,
    
    //TODO SYNC ADD & CLEAR REQ
}

impl Message {
    pub fn is_keep_alive(&self) -> bool {
        if let KeepAlive = self {
            true
        } else { false }
    }

    pub fn is_ack(&self) -> bool {
        if let Ack = self {
            true
        } else { false }
    }

    pub(crate) fn encode(self) -> RawMessage {
        let mut raw_message = [0u8; RAW_MESSAGE_SIZE];
        match self {
            KeepAlive => {}
            Ack => {
                raw_message[0] = 1
            }

            // Client encode
            ClientObstructed { is_obstructed } => {
                raw_message[0] = 64;
                raw_message[1] = is_obstructed as u8
            },
            ClientCabinState { cabin_state } => {
                raw_message[0] = 65;
                raw_message[1..4].copy_from_slice(&cabin_state.encode());
            },
            ClientButtonCall { pressed } => {
                raw_message[0] = 66;
                raw_message[1..4].copy_from_slice(&pressed.encode());
            },
            ClientStopButton { is_pressed } => {
                raw_message[0] = 67;
                raw_message[1] = is_pressed as u8;
            }
            ClientSyncCab { cab_pressed } => {
                raw_message[0] = 68;
                raw_message[1..(N_FLOOR + 1) as usize].copy_from_slice(&cab_pressed.iter().map(|x| *x as u8).collect::<Vec<u8>>())
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
            FullCallLightControl { light_array } => {
                raw_message[0] = 131;
                raw_message[1..RAW_CALL_LIGHT_ARRAY_SIZE + 1].copy_from_slice(&light_array.encode())
            }

            // State flow
            Disconnected => raw_message[0] = 161,
            Connected => raw_message[0] = 162,

            // Synchro
            ControllerSyncState { controller_id, controller_state: state } => {
                raw_message[0] = 193;
                raw_message[1] = controller_id;
                raw_message[2] = state.into();
            },
            ControllerSyncReplace { full_matrix } => {
                raw_message[0] = 194;
                raw_message[1..1 + FCRM_RAW_SIZE].copy_from_slice(&full_matrix.encode());
            }
            ControllerSyncMerge { full_matrix } => {
                raw_message[0] = 195;
                raw_message[1..1 + FCRM_RAW_SIZE].copy_from_slice(&full_matrix.encode());
            }
            ControllerSyncFinish => {
                raw_message[0] = 196
            }
        }

        raw_message
    }
    pub(crate) fn decode_message(raw_message: &RawMessage) -> Result<Self, ()> {
        Ok(match raw_message[0] {
            0 => KeepAlive,
            1 => Ack,

            // Client messages
            64 => ClientObstructed { is_obstructed: raw_message[1] != 0 },
            65 => ClientCabinState { cabin_state: CabinState::decode(&raw_message[1..4]) },
            66 => ClientButtonCall { pressed: CallRequest::decode(&raw_message[1..4]) },
            67 => ClientStopButton { is_pressed: raw_message[1] != 0 },
            68 => {
                let mut cab_pressed = [false; N_FLOOR as usize];
                cab_pressed.copy_from_slice(&raw_message[1..1 + N_FLOOR as usize].iter().map(|x1| *x1 != 0).collect::<Vec<bool>>());
                ClientSyncCab {
                    cab_pressed
                }
            }

            // Controller messages
            128 => ControllerAddress {
                id: raw_message[1],
                state: raw_message[2].into(),
                address: match raw_message[3] {
                    4 => {
                        let mut ip = [0u8; 4];
                        let mut port = [0u8; 2];
                        ip.copy_from_slice(&raw_message[4..8]);
                        port.copy_from_slice(&raw_message[8..8 + size_of::<u16>()]);
                        SocketAddr::from(SocketAddrV4::new(Ipv4Addr::from(ip), u16::from_be_bytes(port)))
                    },
                    6 => {
                        let mut ip = [0u8; 16];
                        let mut port = [0u8; size_of::<u16>()];
                        ip.copy_from_slice(&raw_message[4..20]);
                        port.copy_from_slice(&raw_message[20..20 + size_of::<u16>()]);
                        SocketAddr::from(SocketAddrV6::new(Ipv6Addr::from(ip), u16::from_be_bytes(port), 0, 0))
                    }
                    _ => panic!("TODO")
                }
            },
            129 => LightControl { button: CallRequest::decode(&raw_message[1..4]), is_lit: raw_message[4] != 0 },
            130 => GotoFloor { go_to_floor: raw_message[1] },
            131 => FullCallLightControl { light_array: CallLightArray::decode(&raw_message[1..1 + RAW_CALL_LIGHT_ARRAY_SIZE]) },

            161 => Disconnected,
            162 => Connected,

            193 => ControllerSyncState {
                controller_id: raw_message[1],
                controller_state: raw_message[2].into(),
            },
            194 => {
                let full_matrix = FullControllerRequestsMatrix::decode(
                    &raw_message[1..1 + FCRM_RAW_SIZE]
                );
                ControllerSyncReplace { full_matrix }
            },
            195 => {
                let full_matrix = FullControllerRequestsMatrix::decode(
                    &raw_message[1..1 + FCRM_RAW_SIZE]
                );
                ControllerSyncMerge { full_matrix }
            },
            196 => ControllerSyncFinish,

            code => {
                eprintln!("Bad message code received: {code}");
                Err(())?
            },
        })
    }
}

impl TryFrom<ElevatorEvent> for Message {
    type Error = ();

    fn try_from(value: ElevatorEvent) -> Result<Self, Self::Error> {
        match value {
            ElevatorEvent::CallButton { floor, call } => Ok(
                ClientButtonCall {
                    pressed: match call {
                        CallType::HallUp => Hall { floor, direction: MotorDirection::Up },
                        CallType::HallDown => Hall { floor, direction: MotorDirection::Down },
                        CallType::Cab => Cab { floor }
                    }
                }
            ),
            ElevatorEvent::FloorSensor { .. } => Err(()),
            ElevatorEvent::Obstruction { obstructed } => Ok(
                ClientObstructed { is_obstructed: obstructed }
            ),
            ElevatorEvent::StopButton { stopped } => Ok(
                ClientStopButton { is_pressed: stopped }
            )
        }
    }
}

impl From<CabinState> for Message {
    fn from(cabin_state: CabinState) -> Self {
        ClientCabinState { cabin_state }
    }
}