use log::log_client::ReliableLogSender;
use crate::connection::client_pool::aggregator::{ClientMessage, ClientReceiver, MessageAggregator};
use crate::connection::client_pool::connection_error::{ClientPoolError, ErrorKind};
use crate::connection::connection_handle::handle::{ConnectionHandle, ConnectionIdentifier};
use crate::messages::Message::Authenticated;
use crate::messages::{Message, TimedMessage};
use crate::program_fault::{program_set_to_faulted, Faulted};
use crossbeam_channel::Receiver;
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};

pub enum Target {
    All,
    Specific(ConnectionIdentifier)
}

pub(super) struct Client {
    identifier: ConnectionIdentifier,
    connection: Arc<Mutex<ConnectionHandle>>,
}

impl Client {
    pub(super) fn get_identifier(&self) -> ConnectionIdentifier {
        self.identifier
    }

    pub(super) fn take_message_receiver(&self) -> Receiver<Message> {
        self.connection.lock().unwrap().take_receiver()
    }
}

#[derive(Clone)]
pub struct ClientPool {
    shared_pool: Arc<Mutex<ClientPoolShared>>
}

impl ClientPool {
    pub fn new(faulted: &Faulted, logger: ReliableLogSender, max_client_nb: usize) -> Self {
        let mut pool = Self {
            shared_pool: Arc::new(Mutex::new(ClientPoolShared::new(faulted, logger, max_client_nb))),
        };

        for i in 0..max_client_nb {
            pool.with_client_id(i as u8);
        }

        pool
    }

    fn with_client_id(&mut self, client_id: ConnectionIdentifier) -> &mut Self {
        self.shared_pool.lock().unwrap().with_client_id(client_id);
        self
    }

    pub fn start(self) -> Self {
        self.shared_pool.lock().unwrap().start();
        self
    }

    pub fn connect_stream(&mut self, client_id: ConnectionIdentifier, client_stream: TcpStream ) -> Result<(), ClientPoolError> {
        self.shared_pool.lock().unwrap().connect_stream(client_id, client_stream)
    }

    pub fn send(&mut self, target: Target, message: Message) -> Result<(), ClientPoolError> {
        self.shared_pool.lock().unwrap().send(target, message)
    }

    pub fn take_message_channel(&mut self) -> Receiver<ClientMessage> {
        self.shared_pool.lock().unwrap().receiver.take().unwrap()
    }

    pub fn disconnect_all(&mut self) {
        self.shared_pool.lock().unwrap().disconnect_all();
    }
}

struct ClientPoolShared {
    has_started: bool,
    max_client_nb: usize,
    clients: Vec<Client>,
    logger: ReliableLogSender,
    faulted: Faulted,
    receiver: Option<Receiver<ClientMessage>>,
}

impl ClientPoolShared {
    fn new(faulted: &Faulted, logger: ReliableLogSender, managed_clients: usize) -> Self {
        Self {
            has_started: false,
            max_client_nb: managed_clients,
            clients: Vec::with_capacity(managed_clients),
            faulted: faulted.clone(),
            logger,
            receiver: None,
        }
    }
    fn with_client_id(&mut self, client_id: ConnectionIdentifier) {
        if self.has_started {
            program_set_to_faulted(&self.faulted, "You cannot add clients after starting aggregation");
            return;
        }

        if self.clients.len() >= self.max_client_nb {
            program_set_to_faulted(&self.faulted, "You cannot add more clients");
            return;
        }

        self.clients.push(
            Client {
                identifier: client_id,
                connection: Arc::new(Mutex::new(ConnectionHandle::new_server_connection_handler(
                    self.logger.clone_with_new_prefix(format!("[ClientPool][{client_id}][TCP]")),
                    &self.faulted
                ))),
            }
        );
    }

    fn start(&mut self) {
        if self.has_started {
            program_set_to_faulted(&self.faulted, "Client pool aggregation has already started.");
            return;
        }

        self.has_started = true;
        self.receiver = Some(
            MessageAggregator::init_message_aggregation(
                ClientReceiver::from_clients(&self.clients),
                &self.faulted
            )
        );
    }

    fn send(&mut self, target: Target, message: Message) -> Result<(), ClientPoolError> {
        match target {
            Target::All => {
                let mut all_failed = true;
                for target in &self.clients {
                    let handle = target.connection.lock().unwrap();
                    if handle.is_connected() {
                        all_failed = false;

                        if let Err(_) = Self::send_to(&handle, message) {
                            handle.disconnect();
                        }
                    }
                }

                if all_failed { Err(ClientPoolError::new(ErrorKind::NoConnectedClient)) } else { Ok(()) }
            }
            Target::Specific(target) => {
                match self.clients.iter().find(|client| {
                    client.identifier == target
                }) {
                    None => Err(ClientPoolError::new(ErrorKind::BadIdentifier)),
                    Some(client) => {
                        let handle = client.connection.lock().unwrap();
                        match handle.is_connected() {
                            false => Err(ClientPoolError::new(ErrorKind::ClientIsDisconnected)),
                            true => {
                                if let Err(_) = Self::send_to(&handle, message) {
                                    handle.disconnect();
                                }
                                Ok(())
                            }
                        }
                    }
                }
            }
        }
    }

    fn send_to(target_handle: &ConnectionHandle, message: Message) -> Result<(), ClientPoolError> {
        if let Err(_connection_channel_severed) = target_handle.borrow_sender().send(TimedMessage::of(message)) {
            return Err(ClientPoolError::new(ErrorKind::DeadHandle))
        };
        Ok(())
    }

    fn connect_stream(&mut self, identifier: ConnectionIdentifier, stream: TcpStream) -> Result<(), ClientPoolError> {
        match self.clients.iter_mut().find(|x| {
            x.identifier == identifier
        }) {
            None => {
                let _ = stream.shutdown(Shutdown::Both);
                Err(ClientPoolError::new(ErrorKind::BadIdentifier))
            },
            Some(client) => {
                let client_handle = client.connection.lock().unwrap();

                // Check for already connected instance, this is an error
                if client_handle.is_connected() {
                    let _ = stream.shutdown(Shutdown::Both);
                    Err(ClientPoolError::new(ErrorKind::AlreadyConnected))
                } else {
                    client_handle.connect_to(stream, 0, true);
                    if let Err(error) = Self::send_to(&client_handle, Authenticated) {
                        client_handle.disconnect();
                        Err(error)
                    } else { Ok(()) }
                }
            }
        }
    }

    fn disconnect_all(&mut self) {
        for client in &self.clients {
            client.connection.lock().unwrap().disconnect();
        }
    }
}