use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::ops::Range;
use std::time::Instant;
use driver_rust::elevio::elev::{CallType, ElevatorEvent, MotorDirection};
use Message::{ClientStopButton, ControllerSyncFinish, ControllerSyncMerge, ControllerSyncReplace, Disconnected};
use crate::config::N_FLOOR;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::messages::Message::{Connected, ClientButtonCall, ClientObstructed, ClientCabinState, ControllerAddress, ControllerSyncState, GotoFloor, KeepAlive, LightControl, Ack, ClientSyncCab, FullCallLightControl};
use crate::data_struct::{CabinState, CallLightArray, CallRequest, ControllerState};
use crate::data_struct::CallRequest::{Cab, Hall};

const RAW_PAYLOAD_HEADER_SIZE: usize = 2 + 2 + size_of::<usize>() + size_of::<usize>();
pub const RAW_MESSAGE_SIZE: usize = 32;
pub const RAW_PAYLOAD_SIZE: usize = RAW_PAYLOAD_HEADER_SIZE + RAW_MESSAGE_SIZE;
pub type RawPayload = [u8; RAW_MESSAGE_SIZE + RAW_PAYLOAD_HEADER_SIZE];
pub type RawMessage = [u8; RAW_MESSAGE_SIZE];
pub const UNINIT_RAW_PAYLOAD: RawPayload = [0u8; RAW_PAYLOAD_SIZE];
pub const UNINIT_RAW_MESSAGE: RawMessage = [0u8; RAW_MESSAGE_SIZE];

const SENDER_RANGE: Range<usize> = 0..2;
const DESTINATION_RANGE: Range<usize> = SENDER_RANGE.end..SENDER_RANGE.end + 2;
const ACK_RANGE: Range<usize> = DESTINATION_RANGE.end..DESTINATION_RANGE.end + size_of::<usize>();
const HASH_RANGE: Range<usize> = ACK_RANGE.end..ACK_RANGE.end + size_of::<usize>();
const MESSAGE_RANGE: Range<usize> = HASH_RANGE.end..HASH_RANGE.end + RAW_MESSAGE_SIZE;



#[derive(Debug, Copy, Clone)]
pub struct Payload {
    ack: usize, // 8 bit to store
    hash: usize, // 8 bit to store
    sender: PayloadNode, // 2 bits
    destination: PayloadNode, // 2 bits
    message: Message, // 32 bits, might change.
}

impl Payload {
    pub fn new_uninit(
        message: Message,
        sender: PayloadNode,
        destination: PayloadNode,
    ) -> Payload {
        Self {
            ack: 0,
            hash: 0,
            sender,
            destination,
            message,
        }
    }
    
    pub fn ack_from(payload: Payload) -> Self {
        Self {
            ack: payload.ack,
            hash: payload.hash,
            sender: payload.destination,
            destination: payload.sender,
            message: Ack,
        }
    }

    pub fn set_ack(&mut self, ack: usize) {
        self.ack = ack;
    }

    pub fn set_hash(&mut self, hash: usize) {
        self.hash = hash;
    }
    pub fn ack(&self) -> usize {
        self.ack
    }
    pub fn hash(&self) -> usize {
        self.hash
    }
    pub fn destination(&self) -> PayloadNode {
        self.destination
    }
    pub fn sender(&self) -> PayloadNode {
        self.sender
    }

    pub fn message(&self) -> Message {
        self.message
    }

    pub fn encode(&self) -> RawPayload {
        let mut raw_payload: RawPayload = [0u8; RAW_PAYLOAD_SIZE];

        raw_payload[SENDER_RANGE]
            .copy_from_slice(&self.sender.encode());
        raw_payload[DESTINATION_RANGE]
            .copy_from_slice(&self.destination.encode());
        raw_payload[ACK_RANGE]
            .copy_from_slice(&self.ack.to_be_bytes());
        raw_payload[HASH_RANGE]
            .copy_from_slice(&self.hash.to_be_bytes());
        raw_payload[MESSAGE_RANGE]
            .copy_from_slice(&self.message.encode());

        raw_payload
    }

    pub fn decode_payload(raw_payload: &RawPayload) -> Result<Self, ()> {
        assert_eq!(raw_payload.len(), RAW_PAYLOAD_SIZE);

        let message = Message::decode_message(
            raw_payload[MESSAGE_RANGE].try_into().unwrap()
        )?;

        let sender = PayloadNode::decode(
            raw_payload[SENDER_RANGE].try_into().unwrap()
        );

        let destination = PayloadNode::decode(
            raw_payload[DESTINATION_RANGE].try_into().unwrap()
        );

        let ack = usize::from_be_bytes(
            raw_payload[ACK_RANGE].try_into().unwrap()
        );

        let hash = usize::from_be_bytes(
            raw_payload[HASH_RANGE].try_into().unwrap()
        );

        Ok(Self {
            hash,
            ack,
            sender,
            destination,
            message,
        })
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum PayloadNode {
    Sync,
    Client { client_id: ConnectionIdentifier },
    Controller { controller_id: ConnectionIdentifier }
}

impl PayloadNode {
    fn encode(&self) -> [u8; 2] {
        let mut raw_payload_node = [0u8; 2];
        match self {
            PayloadNode::Sync => raw_payload_node[0] = 1,
            PayloadNode::Client { client_id } => {
                raw_payload_node[0] = 2;
                raw_payload_node[1] = *client_id;
            }
            PayloadNode::Controller { controller_id } => {
                raw_payload_node[0] = 3;
                raw_payload_node[1] = *controller_id;
            }
        }
        raw_payload_node
    }

    fn decode(raw_payload_node: &[u8; 2]) -> Self {
        match raw_payload_node[0] {
            1 => PayloadNode::Sync,
            2 => PayloadNode::Client { client_id: raw_payload_node[1] },
            3 => PayloadNode::Controller { controller_id: raw_payload_node[1] },
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TimedPayload {
    timestamp: Instant,
    payload: Payload,
}

impl TimedPayload {
    pub fn of(payload: Payload) -> Self {
        Self {
            timestamp: Instant::now(),
            payload,
        }
    }

    pub fn from(
        timed_message: TimedMessage,
        sender: PayloadNode,
        destination: PayloadNode,
    ) -> Self {
        Self {
            timestamp: timed_message.timestamp,
            payload: Payload {
                ack: 0,
                hash: 0,
                sender,
                destination,
                message: timed_message.message,
            },
        }
    }

    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    pub fn payload(&self) -> Payload {
        self.payload
    }
}

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
    // TODO NOT HARD CODED FOR 3 CLIENTS AND 4 FLOORS WITH ID 0,1,2
    ControllerSyncReplace {
        client0: [[bool; 3]; 4], // hall up, hall down, cab
        client1: [[bool; 3]; 4],
        client2: [[bool; 3]; 4]
    },
    ControllerSyncMerge {
        client0: [[bool; 3]; 4], // hall up, hall down, cab
        client1: [[bool; 3]; 4],
        client2: [[bool; 3]; 4]
    },
    ControllerSyncFinish,

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
                raw_message[1..(3 * N_FLOOR + 1) as usize].copy_from_slice(&light_array.encode())
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

            // TODO THIS SUCKS
            ControllerSyncReplace {
                client0, client1, client2
            } => {
                raw_message[0] = 194;
                //client0
                raw_message[1] = client0[0][0] as u8; // Going up, floor 0
                raw_message[2] = client0[0][2] as u8; // Cab, floor 0
                raw_message[3..6].copy_from_slice(&client0[1][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); //floor 1
                raw_message[6..9].copy_from_slice(&client0[2][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 2
                raw_message[9..11].copy_from_slice(&client0[3][1..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 3, going down && cab

                //client1
                raw_message[11] = client1[0][0] as u8; // Going up, floor 0
                raw_message[12] = client1[0][2] as u8; // Cab, floor 0
                raw_message[13..16].copy_from_slice(&client1[1][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); //floor 1
                raw_message[16..19].copy_from_slice(&client1[2][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 2
                raw_message[19..21].copy_from_slice(&client1[3][1..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 3, going down && cab

                //client2
                raw_message[21] = client2[0][0] as u8; // Going up, floor 0
                raw_message[22] = client2[0][2] as u8; // Cab, floor 0
                raw_message[23..26].copy_from_slice(&client2[1][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); //floor 1
                raw_message[26..29].copy_from_slice(&client2[2][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 2
                raw_message[29..31].copy_from_slice(&client2[3][1..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 3, going down && cab
            }
            ControllerSyncMerge {
                client0, client1, client2
            } => {
                raw_message[0] = 195;
                //client0
                raw_message[1] = client0[0][0] as u8; // Going up, floor 0
                raw_message[2] = client0[0][2] as u8; // Cab, floor 0
                raw_message[3..6].copy_from_slice(&client0[1][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); //floor 1
                raw_message[6..9].copy_from_slice(&client0[2][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 2
                raw_message[9..11].copy_from_slice(&client0[3][1..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 3, going down && cab

                //client1
                raw_message[11] = client1[0][0] as u8; // Going up, floor 0
                raw_message[12] = client1[0][2] as u8; // Cab, floor 0
                raw_message[13..16].copy_from_slice(&client1[1][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); //floor 1
                raw_message[16..19].copy_from_slice(&client1[2][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 2
                raw_message[19..21].copy_from_slice(&client1[3][1..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 3, going down && cab

                //client2
                raw_message[21] = client2[0][0] as u8; // Going up, floor 0
                raw_message[22] = client2[0][2] as u8; // Cab, floor 0
                raw_message[23..26].copy_from_slice(&client2[1][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); //floor 1
                raw_message[26..29].copy_from_slice(&client2[2][0..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 2
                raw_message[29..31].copy_from_slice(&client2[3][1..3].iter().map(|x| *x as u8).collect::<Vec<u8>>()); // floor 3, going down && cab
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
                let mut cab_pressed= [false; N_FLOOR as usize];
                cab_pressed.copy_from_slice(&raw_message[1..(N_FLOOR + 1) as usize].iter().map(|x1| *x1 != 0).collect::<Vec<bool>>());
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
            131 => FullCallLightControl { light_array: CallLightArray::decode(&raw_message[1..(3 * N_FLOOR + 1) as usize]) },


            161 => Disconnected,
            162 => Connected,

            193 => ControllerSyncState {
                controller_id: raw_message[1],
                controller_state: raw_message[2].into(),
            },
            194 => {
                let mut client0 = [[false; 3]; 4];
                client0[0][0] = raw_message[1] != 0; // Going up, floor 0
                client0[0][2] = raw_message[2] != 0; // Cab, floor 0
                client0[1][0..3].copy_from_slice(&raw_message[3..6].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 1
                client0[2][0..3].copy_from_slice(&raw_message[6..9].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 2
                client0[3][1..3].copy_from_slice(&raw_message[9..11].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 3, going down && cab


                let mut client1 = [[false; 3]; 4];
                client1[0][0] = raw_message[11] != 0; // Going up, floor 0
                client1[0][2] = raw_message[12] != 0; // Cab, floor 0
                client1[1][0..3].copy_from_slice(&raw_message[13..16].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 1
                client1[2][0..3].copy_from_slice(&raw_message[16..19].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 2
                client1[3][1..3].copy_from_slice(&raw_message[19..21].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 3, going down && cab

                let mut client2 = [[false; 3]; 4];
                client2[0][0] = raw_message[21] != 0; // Going up, floor 0
                client2[0][2] = raw_message[22] != 0; // Cab, floor 0
                client2[1][0..3].copy_from_slice(&raw_message[23..26].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 1
                client2[2][0..3].copy_from_slice(&raw_message[26..29].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 2
                client2[3][1..3].copy_from_slice(&raw_message[29..31].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 3, going down && cab

                ControllerSyncReplace {
                    client0,
                    client1,
                    client2,
                }
            },
            195 => {
                let mut client0 = [[false; 3]; 4];
                client0[0][0] = raw_message[1] != 0; // Going up, floor 0
                client0[0][2] = raw_message[2] != 0; // Cab, floor 0
                client0[1][0..3].copy_from_slice(&raw_message[3..6].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 1
                client0[2][0..3].copy_from_slice(&raw_message[6..9].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 2
                client0[3][1..3].copy_from_slice(&raw_message[9..11].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 3, going down && cab


                let mut client1 = [[false; 3]; 4];
                client1[0][0] = raw_message[11] != 0; // Going up, floor 0
                client1[0][2] = raw_message[12] != 0; // Cab, floor 0
                client1[1][0..3].copy_from_slice(&raw_message[13..16].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 1
                client1[2][0..3].copy_from_slice(&raw_message[16..19].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 2
                client1[3][1..3].copy_from_slice(&raw_message[19..21].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 3, going down && cab

                let mut client2 = [[false; 3]; 4];
                client2[0][0] = raw_message[21] != 0; // Going up, floor 0
                client2[0][2] = raw_message[22] != 0; // Cab, floor 0
                client2[1][0..3].copy_from_slice(&raw_message[23..26].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 1
                client2[2][0..3].copy_from_slice(&raw_message[26..29].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 2
                client2[3][1..3].copy_from_slice(&raw_message[29..31].iter().map(|x| *x != 0).collect::<Vec<bool>>()); // floor 3, going down && cab

                ControllerSyncMerge {
                    client0,
                    client1,
                    client2,
                }
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