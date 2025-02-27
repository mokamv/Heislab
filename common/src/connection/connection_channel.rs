use std::cmp::PartialEq;
use crate::messages::Message;
use std::sync::{Arc, Mutex};
use crossbeam_channel::{unbounded, Receiver, Sender};

pub struct ConnectionTransmitters {
    message_sender: Option<Sender<Message>>,
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
                last_sent_status: Arc::new(Mutex::new(AliveStatus::Disconnected)),
            }
        }
    }

    pub(super) fn get_alive_notifier_instance(&self) -> AliveStatusNotifier {
        AliveStatusNotifier {
            is_alive_sender: self.alive_status_notifier.is_alive_sender.clone(),
            last_sent_status: self.alive_status_notifier.last_sent_status.clone(),
        }
    }

    pub(super) fn notify_status(&self, status: AliveStatus) {
        self.alive_status_notifier.notify_status(status)
    }

    pub(super) fn take_status(&mut self) -> Receiver<AliveStatus> {
        self.is_alive_receiver.take().unwrap()
    }

    pub(super) fn take_receiver(&mut self) -> Receiver<Message> {
        self.message_receiver.take().unwrap()
    }

    pub(super) fn take_sender(&mut self) -> Sender<Message> {
        self.message_sender.take().unwrap()
    }

    pub(super) fn borrow_sender(&self) -> &Sender<Message> {
        self.message_sender.as_ref().unwrap()
    }

    pub(super) fn borrow_receiver(&self) -> &Receiver<Message> {
        self.message_receiver.as_ref().unwrap()
    }

    pub(super) fn populate(&mut self, message_sender: Sender<Message>, message_receiver: Receiver<Message>) {
        self.message_sender = Some(message_sender);
        self.message_receiver = Some(message_receiver);
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum AliveStatus {
    Disconnected,
    Connected,
    ConnectedAndAuthenticated
}

pub struct AliveStatusNotifier {
    is_alive_sender: Sender<AliveStatus>,
    last_sent_status: Arc<Mutex<AliveStatus>>
}

impl AliveStatusNotifier {
    pub(super) fn notify_status(&self, status: AliveStatus) {
        let mut current_connect_status = self.last_sent_status.lock().unwrap();

        if *current_connect_status != status {
            *current_connect_status = status;
            if let Err(_) = self.is_alive_sender.send(status) {
                println!("FAILED");
            }
        }
        drop(current_connect_status);
    }
}

