use crossbeam_channel::Sender;
use crate::messages::{Message, TimedMessage};

#[derive(Clone)]
pub struct MessageSender {
    sender: Sender<TimedMessage>
}

impl MessageSender {
    pub fn from(sender: Sender<TimedMessage>) -> Self {
        Self { sender }
    }

    pub fn send<T>(&self, message: T)
    where T: Into<Message>
    {
        self.sender.send(
            TimedMessage::of(message.into())
        ).unwrap()
    }
}