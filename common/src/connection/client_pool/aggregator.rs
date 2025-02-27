use crate::messages::Message;
use crate::program_fault::Faulted;
use crossbeam_channel::{unbounded, Receiver, RecvError, Select, SendError, Sender};
use std::thread::spawn;
use std::time::Duration;
use crate::connection::client_pool::client_pool::Client;
use crate::connection::connection_channel::AliveStatus;

#[derive(Debug)]
pub struct ClientMessage {
    pub identifier: u32,
    pub message: MessageType
}

#[derive(Debug)]
pub enum MessageType {
    Data(Message),
    Connected,
    Disconnected
}

#[derive(Debug)]
enum ReceiverType {
    StatusReceiver(Receiver<AliveStatus>),
    MessageReceiver(Receiver<Message>)
}

struct AggregatedReceiver {
    selection_index: usize,
    internal_client_identifier: u32,
    internal_receiver: ReceiverType,
}

pub(super) struct ClientReceiver {
    internal_client_identifier: u32,
    internal_receiver: Receiver<Message>,
    internal_status_receiver: Receiver<AliveStatus>
}

impl ClientReceiver {
    pub(super) fn from_clients(clients: &Vec<Client>) -> Vec<ClientReceiver> {
        let mut clients_receivers: Vec<ClientReceiver> = vec![];
        for client in clients {
            clients_receivers.push(ClientReceiver {
                internal_client_identifier: client.get_identifier(),
                internal_receiver: client.take_message_receiver(),
                internal_status_receiver: client.take_status_receiver(),
            })
        }
        clients_receivers
    }
}

type MessageReceivers = Vec<AggregatedReceiver>;

pub(super) struct MessageAggregator<'a> {
    selector: Select<'a>,
    receivers: &'a MessageReceivers,
    global_message_sender: Sender<ClientMessage>
}
impl<'a> MessageAggregator<'a> {
    pub(super) fn init_message_aggregation(clients_receiver: Vec<ClientReceiver>, faulted: &Faulted) -> Receiver<ClientMessage> {
        let faulted = faulted.clone();
        let (global_message_sender, global_message_receiver) = unbounded();

        spawn(move || {
            let mut selector = Select::new();
            let receivers: MessageReceivers = Self::init_receivers(clients_receiver);

            // Add receivers to selector
            for receiver in receivers.iter() {
                let index = match &receiver.internal_receiver {
                    ReceiverType::StatusReceiver(recv) => selector.recv(recv),
                    ReceiverType::MessageReceiver(recv) => selector.recv(recv)
                };

                assert_eq!(index, receiver.selection_index)
            }

            let mut aggregator = MessageAggregator {
                selector,
                receivers: &receivers,
                global_message_sender,
            };

            while !*faulted.lock().unwrap() {
                if let
                    Err(AggregatorError::SeveredGlobalAggregator)
                    | Err(AggregatorError::NoReceiver) = aggregator.try_select() { break; }
            }
        });

        global_message_receiver
    }

    fn init_receivers(clients_receiver: Vec<ClientReceiver>) -> MessageReceivers {
        let mut receivers: MessageReceivers = vec![];

        let mut index = 0;
        for client in clients_receiver {
            receivers.push(AggregatedReceiver {
                selection_index: index,
                internal_client_identifier: client.internal_client_identifier,
                internal_receiver: ReceiverType::MessageReceiver(client.internal_receiver),
            });

            receivers.push(AggregatedReceiver {
                selection_index: index + 1,
                internal_client_identifier: client.internal_client_identifier,
                internal_receiver: ReceiverType::StatusReceiver(client.internal_status_receiver),
            });

            index += 2
        }

        receivers
    }

    fn try_select(&mut self) -> Result<(), AggregatorError> {
        match self.selector.select_timeout(Duration::from_millis(50)) {
            Ok(operation) => {
                let aggregated_receiver = self.borrow_receiver(operation.index())?;
                let message = match &aggregated_receiver.internal_receiver {
                    ReceiverType::StatusReceiver(status_recv) => {
                        match operation.recv(status_recv)? {
                            AliveStatus::Disconnected => MessageType::Disconnected,
                            AliveStatus::Connected | AliveStatus::ConnectedAndAuthenticated => //TODO CHECK
                                MessageType::Connected
                        }
                    }

                    ReceiverType::MessageReceiver(message_recv) => {
                        MessageType::Data(operation.recv(message_recv)?)
                    }
                };
                Ok(self.global_message_sender.send(ClientMessage {
                    identifier: aggregated_receiver.internal_client_identifier,
                    message,
                })?)
            }
            Err(_timeout_error) => Err(AggregatorError::AggregationTimeout)
        }
    }

    fn borrow_receiver(&self, operation_index: usize) -> Result<&AggregatedReceiver, AggregatorError> {
        match self.receivers.iter().find(|x| {
            x.selection_index == operation_index
        }) {
            None => Err(AggregatorError::NoReceiver),
            Some(aggregated_receiver) => Ok(aggregated_receiver)
        }
    }
}

enum AggregatorError {
    NoReceiver,
    SeveredGlobalAggregator,
    SeveredAggregated,
    AggregationTimeout
}

impl From<RecvError> for AggregatorError {
    fn from(value: RecvError) -> Self {
        AggregatorError::SeveredAggregated
    }
}

impl From<SendError<ClientMessage>> for AggregatorError {
    fn from(value: SendError<ClientMessage>) -> Self {
        AggregatorError::SeveredGlobalAggregator
    }
}
