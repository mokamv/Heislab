use crate::connection::constants::UDP_RETRY;
use crate::messages::{Payload, PayloadNode};
use crossbeam_channel::{after, never, Receiver};
use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

pub const ACK_IGNORE: usize = usize::MAX;
pub type MapperKey = ( PayloadNode, PayloadNode );

struct MapperValue {
    last_recv_ack: usize,
    ack_send_counter: usize,
    hash: usize,
    retry_recv: Receiver<Instant>,
    address: SocketAddr,
    payloads: VecDeque<Payload>
}

pub struct UdpAckSocket {
    udp_socket: UdpSocket,
    routes: HashMap<MapperKey, MapperValue>,
    global_ack: usize
}

impl UdpAckSocket {
    pub fn from(udp_socket: UdpSocket) -> Self {
        Self {
            udp_socket,
            routes: Default::default(),
            global_ack: 0
        }
    }

    pub fn get_retry_receivers(&self) -> Vec<(PayloadNode, PayloadNode, Receiver<Instant>)> {
        let mut receivers = vec![];
        for (key, recv) in &self.routes {
            receivers.push((
                key.0,
                key.1,
                recv.retry_recv.clone()
            ))
        };
        receivers
    }

    pub fn send_to_ignore_ack(
        &mut self,
        mut payload: Payload
    ) {
        let key: MapperKey = (payload.sender(), payload.destination());
        let route_value = self
            .routes
            .get(&key);

        if let Some(route_value) = route_value {
            self.global_ack += 1;
            payload.set_ack(self.global_ack);
            payload.set_hash(route_value.hash);

            // println!("SENDING KEEPALIVE ACK {} HASH {}", payload.ack(), payload.hash());

            let _ = self.udp_socket.send_to(
                &payload.encode(),
                route_value.address,
            );
        }
    }

    pub fn send_to(
        &mut self,
        mut payload: Payload,
    ) {
        debug_assert!(!payload.message().is_keep_alive());

        let key: MapperKey = (payload.sender(), payload.destination());
        debug_assert!(self.routes.get(&key).is_some());

        let route_value = self
            .routes
            .get_mut(&key)
            .unwrap();

        // Add payload to the queue
        route_value.ack_send_counter += 1;
        payload.set_ack(route_value.ack_send_counter);
        payload.set_hash(route_value.hash);
        route_value.payloads
            .push_back(payload);

        if route_value.payloads.len() == 1 {
            //TODO HANDLE ERROR
            let _ = self.udp_socket.send_to(
                &payload.encode(),
                route_value.address
            );
            route_value.retry_recv = after(UDP_RETRY);
            // println!("SENDING: {:?} at {:?} to {}", payload, Instant::now(), route_value.address);
        }
    }

    pub fn resend(
        &mut self,
        sender: PayloadNode,
        destination: PayloadNode
    ) {
        let key: MapperKey = (sender, destination);
        debug_assert!(self.routes.get(&key).is_some());

        let route_value = self
            .routes
            .get_mut(&key)
            .unwrap();

        let resent_payload = route_value
            .payloads
            .front()
            .unwrap();

        println!("RESENDING: {:?} at {:?}", resent_payload, Instant::now());

        // TODO ERROR HANDLE
        let _ = self.udp_socket.send_to(
            &resent_payload.encode(),
            route_value.address
        );
        route_value.retry_recv = after(UDP_RETRY);
    }

    pub fn send_acknowledge_for(
        &mut self,
        payload_to_ack: Payload
    ) {
        // println!("SENDING ACKNOWLEDGING: {}", payload_to_ack.ack());
        debug_assert!(!payload_to_ack.message().is_ack());

        let ack_payload = Payload::ack_from(payload_to_ack);

        let key = (payload_to_ack.destination(), payload_to_ack.sender());

        let route_value = self
            .routes
            .get_mut(&key);

        // Ignore ack when the route doesn't exist at the moment.
        if route_value.is_none() {
            return;
        }

        let address = route_value
            .unwrap()
            .address;

        let _ = self.udp_socket.send_to(
            &ack_payload.encode(),
            address
        );
    }

    pub fn acknowledged_by(
        &mut self,
        ack_payload: Payload
    ) {
        // println!("ACKNOWLEDGING: {} at {:?}", ack_payload.ack(), Instant::now());

        debug_assert!(ack_payload.message().is_ack());
        // Need to invert the sender/destination since this is a response
        let key = (ack_payload.destination(), ack_payload.sender());

        let route_value = self
            .routes
            .get_mut(&key);

        // Ignore ack when the route doesn't exist at the moment.
        if route_value.is_none() {
            return;
        }

        let route_value = route_value.unwrap();
        // Ignore non-matching ack, that is probably an older ack.
        if route_value.hash != ack_payload.hash() {
            return
        }

        // Check for the current sent payload and match it against the ack.
        if let Some(current_payload) = route_value.payloads.front() {
            if current_payload.ack() == ack_payload.ack() {
                // Remove current packet
                let _ = route_value.payloads.pop_front();
                route_value.retry_recv = never();

                // Check if another payload is available.
                if route_value.payloads.is_empty() {
                    return;
                }

                // Send the next payload.
                let next_payload = route_value.payloads.front().unwrap();
                // TODO ERROR HANDLE
                let _ = self.udp_socket.send_to(
                    &next_payload.encode(),
                    route_value.address
                );

                // println!("SENDING FROM QUEUE: {} at {:?}", next_payload.ack(), Instant::now());
                route_value.retry_recv = after(UDP_RETRY);
            }
        }
    }

    pub fn has_payload_already_been_received(
        &mut self,
        payload: Payload
    ) -> bool {
        let key = (payload.destination(), payload.sender());

        let route_value = self
            .routes
            .get_mut(&key);

        // drop message from nonexistent routes.
        if route_value.is_none() {
            return false;
        }

        let route_value = route_value.unwrap();
        if route_value.last_recv_ack < payload.ack() {
            route_value.last_recv_ack = payload.ack();
            false
        } else {
            true
        }
    }

    pub fn create_route(
        &mut self,
        sender: PayloadNode,
        destination: PayloadNode,
        address: SocketAddr,
    ) {
        let key: MapperKey = (sender, destination);
        debug_assert!(self.routes.get(&key).is_none());
        let _ = self.routes.insert(
            key,
            MapperValue {
                hash: rand::random::<u64>() as usize,
                ack_send_counter: 0,
                last_recv_ack: 0,
                address,
                payloads: Default::default(),
                retry_recv: never(),
            }
        );
    }

    pub fn clear_route(
        &mut self,
        sender: PayloadNode,
        destination: PayloadNode
    ) {
        let key: MapperKey = (sender, destination);
        debug_assert!(self.routes.get(&key).is_some());
        let _ = self.routes.remove(&key);
    }
}