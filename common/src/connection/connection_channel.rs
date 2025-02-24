use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use crate::messages::RawMessage;

pub struct ConnectionTransmitters {
    message_sender: Option<Sender<RawMessage>>,
    message_receiver: Option<Receiver<RawMessage>>,
    is_alive_receiver: Option<Receiver<bool>>,
    alive_status_notifier: AliveStatusNotifier
}

impl ConnectionTransmitters {
    pub(super) fn init() -> Self {
        let (tx, rx) = channel();

        Self {
            message_sender: None,
            message_receiver: None,
            is_alive_receiver: Some(rx),
            alive_status_notifier: AliveStatusNotifier {
                is_alive_sender: tx,
                last_sent_status: Arc::new(Mutex::new(false)),
            }
        }
    }

    pub(super) fn alive_status_notifier(&self) -> AliveStatusNotifier {
        AliveStatusNotifier {
            is_alive_sender: self.alive_status_notifier.is_alive_sender.clone(),
            last_sent_status: self.alive_status_notifier.last_sent_status.clone(),
        }
    }

    pub(super) fn take_status(&mut self) -> Receiver<bool> {
        self.is_alive_receiver.take().unwrap()
    }

    pub(super) fn take_receiver(&mut self) -> Receiver<RawMessage> {
        self.message_receiver.take().unwrap()
    }

    pub(super) fn take_sender(&mut self) -> Sender<RawMessage> {
        self.message_sender.take().unwrap()
    }

    pub(super) fn borrow_sender(&self) -> &Sender<RawMessage> {
        self.message_sender.as_ref().unwrap()
    }

    pub(super) fn borrow_receiver(&self) -> &Receiver<RawMessage> {
        self.message_receiver.as_ref().unwrap()
    }

    pub(super) fn populate(&mut self, message_sender: Sender<RawMessage>, message_receiver: Receiver<RawMessage>) {
        self.message_sender = Some(message_sender);
        self.message_receiver = Some(message_receiver);
    }
}

pub struct AliveStatusNotifier {
    is_alive_sender: Sender<bool>,
    last_sent_status: Arc<Mutex<bool>>
}

impl AliveStatusNotifier {
    pub(super) fn is_connected(&self, connected: bool) {
        if *self.last_sent_status.lock().unwrap() != connected {
            *self.last_sent_status.lock().unwrap() = connected;
            if let Err(_) = self.is_alive_sender.send(connected) {
                println!("FAILED");
            }
        }
    }
}

