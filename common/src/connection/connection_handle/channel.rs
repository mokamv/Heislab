use std::cmp::PartialEq;
use crate::messages::{Message, TimedMessage};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::connection::connection_handle::channel::AliveValue::{Connected, ConnectedAndAuthenticated, Disconnected};

pub struct ConnectionTransmitters {
    message_sender: Option<Sender<TimedMessage>>,
    message_receiver: Option<Receiver<Message>>,
    is_alive_receiver: Option<Receiver<AliveStatus>>,
    alive_status_notifier: AliveStatusNotifier
}

impl ConnectionTransmitters {
    pub(super) fn init() -> Self {
        let (tx, rx) = unbounded();

        Self {
            message_sender: None,
            message_receiver: None,
            is_alive_receiver: Some(rx),
            alive_status_notifier: AliveStatusNotifier {
                is_alive_sender: tx,
                is_currently_alive: Arc::new(Mutex::new(AliveStatus::of(Disconnected))),
            }
        }
    }

    pub(super) fn get_alive_notifier_instance(&self) -> AliveStatusNotifier {
        self.alive_status_notifier.clone()
    }
    
    pub (super) fn disconnected(&self) {
        self.alive_status_notifier.disconnected()
    }

    pub (super) fn connected(&self) {
        self.alive_status_notifier.connected()
    }

    pub(super) fn authenticated(&self) {
        self.alive_status_notifier.authenticated();
    }


    pub(super) fn take_status(&mut self) -> Receiver<AliveStatus> {
        self.is_alive_receiver.take().unwrap()
    }

    pub(super) fn take_receiver(&mut self) -> Receiver<Message> {
        self.message_receiver.take().unwrap()
    }

    pub(super) fn take_sender(&mut self) -> Sender<TimedMessage> {
        self.message_sender.take().unwrap()
    }

    pub(super) fn borrow_sender(&self) -> &Sender<TimedMessage> {
        self.message_sender.as_ref().unwrap()
    }

    pub(super) fn borrow_receiver(&self) -> &Receiver<Message> {
        self.message_receiver.as_ref().unwrap()
    }

    pub(super) fn populate(&mut self, message_sender: Sender<TimedMessage>, message_receiver: Receiver<Message>) {
        self.message_sender = Some(message_sender);
        self.message_receiver = Some(message_receiver);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AliveStatus {
    value: AliveValue,
    timestamp: Instant
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AliveValue {
    Connected,
    Disconnected,
    ConnectedAndAuthenticated
}

impl AliveStatus {
    pub(crate) fn of(value: AliveValue) -> Self {
        Self {
            value,
            timestamp: Instant::now(),
        }
    }

    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    pub fn value(&self) -> AliveValue {
        self.value
    }

    pub fn is_connected(&self) -> bool {
        self.value != Disconnected
    }

    pub fn is_valid_for(&self, timestamp :Instant) -> bool {
        self.is_connected() && timestamp > self.timestamp
    }
}

pub struct AliveStatusNotifier {
    is_alive_sender: Sender<AliveStatus>,
    is_currently_alive: Arc<Mutex<AliveStatus>>
}

impl Clone for AliveStatusNotifier {
    fn clone(&self) -> Self {
        AliveStatusNotifier {
            is_alive_sender: self.is_alive_sender.clone(),
            is_currently_alive: self.is_currently_alive.clone(),
        }
    }
}

impl AliveStatusNotifier {
    fn notify_status(&self, value: AliveValue) {
        let mut current_connect_status = self.is_currently_alive.lock().unwrap();
        if current_connect_status.value != value {
            let status = AliveStatus::of(value);
            *current_connect_status = status;
            if let Err(_) = self.is_alive_sender.send(status) {
                println!("FAILED"); //TODO ERROR
            }
        }
    }

    pub(super) fn connected(&self) {
        self.notify_status(Connected)
    }

    pub(super) fn disconnected(&self) {
        self.notify_status(Disconnected)
    }

    pub(super) fn authenticated(&self) {
        self.notify_status(ConnectedAndAuthenticated)
    }
}

