use crate::connection::client_pool::client_pool::Client;
use crate::connection::connection_handle::handle::ConnectionIdentifier;
use crate::messages::Message;
use crossbeam_channel::{unbounded, Receiver, RecvError, Select, SendError, Sender};
use std::thread::spawn;
use std::time::Duration;
use faulted::is_faulted;

#[derive(Debug)]
pub struct ClientMessage {
    pub identifier: ConnectionIdentifier,
    pub content: Message
}

struct AggregatedReceiver {
    selection_index: usize,
    internal_client_identifier: ConnectionIdentifier,
    internal_receiver: Receiver<Message>,
}

pub(super) struct ClientReceiver {
    internal_client_identifier: ConnectionIdentifier,
    internal_receiver: Receiver<Message>,
}

impl ClientReceiver {
    pub(super) fn from_clients(clients: &Vec<Client>) -> Vec<ClientReceiver> {
        let mut clients_receivers: Vec<ClientReceiver> = vec![];
        for client in clients {
            clients_receivers.push(ClientReceiver {
                internal_client_identifier: client.get_identifier(),
                internal_receiver: client.take_message_receiver(),
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
    pub(super) fn init_message_aggregation(clients_receiver: Vec<ClientReceiver>) -> Receiver<ClientMessage> {
        let (global_message_sender, global_message_receiver) = unbounded();

        spawn(move || {
            let mut selector = Select::new();
            let receivers: MessageReceivers = Self::init_receivers(clients_receiver);

            // Add receivers to selector
            for receiver in receivers.iter() {
                let index = selector.recv(&receiver.internal_receiver);
                assert_eq!(index, receiver.selection_index)
            }

            let mut aggregator = MessageAggregator {
                selector,
                receivers: &receivers,
                global_message_sender,
            };

            while !is_faulted() {
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
                internal_receiver: client.internal_receiver,
            });

            index += 1
        }

        receivers
    }

    fn try_select(&mut self) -> Result<(), AggregatorError> {
        match self.selector.select_timeout(Duration::from_millis(50)) {
            Ok(operation) => {
                let aggregated_receiver = self.borrow_receiver(operation.index())?;
                let message = operation.recv(&aggregated_receiver.internal_receiver)?;

                Ok(self.global_message_sender.send(ClientMessage {
                    identifier: aggregated_receiver.internal_client_identifier,
                    content: message,
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
    fn from(_value: RecvError) -> Self {
        AggregatorError::SeveredAggregated
    }
}

impl From<SendError<ClientMessage>> for AggregatorError {
    fn from(_value: SendError<ClientMessage>) -> Self {
        AggregatorError::SeveredGlobalAggregator
    }
}
