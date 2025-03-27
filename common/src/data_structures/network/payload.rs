use std::ops::Range;
use std::time::Instant;
use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::data_structures::network::message::{Message, TimedMessage, RAW_MESSAGE_SIZE};
use crate::data_structures::network::message::Message::Ack;

const RAW_PAYLOAD_HEADER_SIZE: usize = 2 + 2 + size_of::<usize>() + size_of::<usize>();
pub const RAW_PAYLOAD_SIZE: usize = RAW_PAYLOAD_HEADER_SIZE + RAW_MESSAGE_SIZE;
pub type RawPayload = [u8; RAW_MESSAGE_SIZE + RAW_PAYLOAD_HEADER_SIZE];

pub const UNINIT_RAW_PAYLOAD: RawPayload = [0u8; RAW_PAYLOAD_SIZE];


const SENDER_RANGE: Range<usize> = 0..2;
const DESTINATION_RANGE: Range<usize> = SENDER_RANGE.end..SENDER_RANGE.end + 2;
const ACK_RANGE: Range<usize> = DESTINATION_RANGE.end..DESTINATION_RANGE.end + size_of::<usize>();
const HASH_RANGE: Range<usize> = ACK_RANGE.end..ACK_RANGE.end + size_of::<usize>();
const MESSAGE_RANGE: Range<usize> = HASH_RANGE.end..HASH_RANGE.end + RAW_MESSAGE_SIZE;



#[derive(Debug, Copy, Clone)]
pub struct NetworkPayload {
    ack: usize, // 8 bit to store
    hash: usize, // 8 bit to store
    sender: NetworkPayloadNode, // 2 bits
    destination: NetworkPayloadNode, // 2 bits
    message: Message, // 32 bits, might change.
}

impl NetworkPayload {
    pub fn new_uninit(
        message: Message,
        sender: NetworkPayloadNode,
        destination: NetworkPayloadNode,
    ) -> NetworkPayload {
        Self {
            ack: 0,
            hash: 0,
            sender,
            destination,
            message,
        }
    }

    pub fn ack_from(payload: NetworkPayload) -> Self {
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
    pub fn destination(&self) -> NetworkPayloadNode {
        self.destination
    }
    pub fn sender(&self) -> NetworkPayloadNode {
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

        let sender = NetworkPayloadNode::decode(
            raw_payload[SENDER_RANGE].try_into().unwrap()
        );

        let destination = NetworkPayloadNode::decode(
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
pub enum NetworkPayloadNode {
    Sync,
    Client { client_id: ConnectionIdentifier },
    Controller { controller_id: ConnectionIdentifier }
}

impl NetworkPayloadNode {
    fn encode(&self) -> [u8; 2] {
        let mut raw_payload_node = [0u8; 2];
        match self {
            NetworkPayloadNode::Sync => raw_payload_node[0] = 1,
            NetworkPayloadNode::Client { client_id } => {
                raw_payload_node[0] = 2;
                raw_payload_node[1] = *client_id;
            }
            NetworkPayloadNode::Controller { controller_id } => {
                raw_payload_node[0] = 3;
                raw_payload_node[1] = *controller_id;
            }
        }
        raw_payload_node
    }

    fn decode(raw_payload_node: &[u8; 2]) -> Self {
        match raw_payload_node[0] {
            1 => NetworkPayloadNode::Sync,
            2 => NetworkPayloadNode::Client { client_id: raw_payload_node[1] },
            3 => NetworkPayloadNode::Controller { controller_id: raw_payload_node[1] },
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TimedPayload {
    timestamp: Instant,
    payload: NetworkPayload,
}

impl TimedPayload {
    pub fn of(payload: NetworkPayload) -> Self {
        Self {
            timestamp: Instant::now(),
            payload,
        }
    }

    pub fn from(
        timed_message: TimedMessage,
        sender: NetworkPayloadNode,
        destination: NetworkPayloadNode,
    ) -> Self {
        Self {
            timestamp: timed_message.timestamp,
            payload: NetworkPayload {
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

    pub fn payload(&self) -> NetworkPayload {
        self.payload
    }
}