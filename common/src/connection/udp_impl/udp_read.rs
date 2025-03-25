use std::io::{Error, ErrorKind};
use std::io::ErrorKind::{TimedOut, WouldBlock};
use std::net::{SocketAddr, UdpSocket};
use crate::connection::udp_impl::udp_read::UdpReadError::{InvalidMessageSize, NonControllerFrame, ReadBlocked, SocketError};
use crate::messages::{Message, RawMessage, UNINIT_RAW_MESSAGE, RAW_MESSAGE_SIZE, RAW_PAYLOAD_SIZE, TimedPayload, RawPayload, UNINIT_RAW_PAYLOAD, Payload};

#[derive(Debug)]
pub enum UdpReadError {
    SocketError,
    InvalidMessage,
    InvalidMessageSize,
    NonControllerFrame,
    ReadBlocked,
}

impl From<()> for UdpReadError {
    fn from(_: ()) -> Self {
        UdpReadError::InvalidMessage
    }
}

impl Into<Error> for UdpReadError {
    fn into(self) -> Error {
        Error::from(ErrorKind::ConnectionRefused)
    }
}

pub fn upd_read_one_bc_message_on_port(
    udp_socket: &mut UdpSocket,
    port: u16
) -> Result<Message, UdpReadError> {
    let mut raw_message_buffer: RawMessage = UNINIT_RAW_MESSAGE;

    let read_result = udp_socket.recv_from(&mut raw_message_buffer);

    match read_result {
        // A potential message has been received
        Ok((RAW_MESSAGE_SIZE, address)) if address.port() == port => Ok(
            Message::decode_message(&raw_message_buffer)?
        ),

        // Ignore non controller message.
        Ok((RAW_MESSAGE_SIZE, _)) => Err(NonControllerFrame),

        // Not enough data has been pulled to constitute a message, this is an error
        Ok(_) => Err(InvalidMessageSize),
        // Explicit error
        Err(error) if error.kind() == WouldBlock || error.kind() == TimedOut => Err(ReadBlocked),
        Err(_error) => Err(SocketError)
    }
}

pub fn udp_read_one_message(
    udp_socket: &mut UdpSocket
) -> Result<(TimedPayload, SocketAddr), UdpReadError> {
    let mut raw_message_buffer: RawPayload = UNINIT_RAW_PAYLOAD;

    let read_result = udp_socket.recv_from(&mut raw_message_buffer);

    match read_result {
        // A potential message has been received
        Ok((RAW_PAYLOAD_SIZE, address))  => {
            Ok((
                TimedPayload::of(
                    Payload::decode_payload(&raw_message_buffer)?
                ),
                address
            ))
        },

        // Not enough data has been pulled to constitute a message, this is an error
        Ok(_) => Err(InvalidMessageSize),
        // Explicit error
        Err(error) if error.kind() == WouldBlock || error.kind() == TimedOut => Err(ReadBlocked),
        Err(_error) => Err(SocketError)
    }
}

pub fn udp_read_one_message_from_connected_socket(
    udp_socket: &mut UdpSocket
) -> Result<TimedPayload, UdpReadError> {
    let mut raw_message_buffer: RawPayload = UNINIT_RAW_PAYLOAD;

    let read_result = udp_socket.recv(&mut raw_message_buffer);
    match read_result {
        // A potential message has been received
        Ok(RAW_PAYLOAD_SIZE) => Ok(TimedPayload::of(
            Payload::decode_payload(&raw_message_buffer)?
        )),
        // Not enough data has been pulled to constitute a message, this is an error
        Ok(_) => Err(InvalidMessageSize),
        // Explicit error
        Err(error) if error.kind() == WouldBlock || error.kind() == TimedOut => Err(ReadBlocked),
        Err(error) => {
            println!("On read: {error:?}");
            Err(SocketError)
        }
    }
}