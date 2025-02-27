use crate::connection::client_pool::aggregator::{ClientMessage, ClientReceiver, MessageAggregator};
use crate::connection::client_pool::connection_error::{ClientPoolError, ErrorKind};
use crate::connection::connection_channel::AliveStatus;
use crate::connection::connection_handle::{ConnectionHandle, ConnectionState};
use crate::log::log_client::ReliableLogSender;
use crate::messages::Message;
use crate::messages::Message::Authenticated;
use crate::program_fault::{program_set_to_faulted, Faulted};
use crossbeam_channel::Receiver;
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use crate::connection::constants::IDENTIFICATION_TIMEOUT;

pub enum Target {
    All,
    Specific(u32)
}

pub(super) struct Client {
    identifier: u32,
    connection: Arc<Mutex<ConnectionHandle>>,
}

impl Client {
    pub(super) fn get_identifier(&self) -> u32 {
        self.identifier
    }

    pub(super) fn take_message_receiver(&self) -> Receiver<Message> {
        self.connection.lock().unwrap().take_receiver()
    }

    pub(super) fn take_status_receiver(&self) -> Receiver<AliveStatus> {
        self.connection.lock().unwrap().take_status()
    }
}

#[derive(Clone)]
pub struct ClientPool {
    shared_pool: Arc<Mutex<ClientPoolShared>>
}

impl ClientPool {
    pub fn new(faulted: &Faulted, logger: ReliableLogSender, max_client_nb: usize) -> Self {
        Self {
            shared_pool: Arc::new(Mutex::new(ClientPoolShared::new(faulted, logger, max_client_nb))),
        }
    }

    pub fn with_client_id(&mut self, client_id: u32) -> &mut Self {
        self.shared_pool.lock().unwrap().with_client_id(client_id);
        self
    }

    pub fn start(self) -> Self {
        self.shared_pool.lock().unwrap().start();
        self
    }

    pub fn handle_new_connection(&mut self, connection: ConnectionHandle) -> Result<(), ClientPoolError> {
        let (client_id, client_stream) = ClientPoolShared::handle_identification(connection)?;
        self.shared_pool.lock().unwrap().enable_connection(client_id, client_stream)
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
    fn with_client_id(&mut self, client_id: u32) {
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
        if let Err(_connection_channel_severed) = target_handle.borrow_sender().send(message) {
            return Err(ClientPoolError::new(ErrorKind::DeadHandle))
        };
        Ok(())
    }

    fn handle_identification(potential_handle: ConnectionHandle) -> Result<(u32, TcpStream), ClientPoolError> {
        let message_receiver = potential_handle.borrow_receiver();
        match message_receiver.recv_timeout(IDENTIFICATION_TIMEOUT) {
            Ok(Message::ClientAuth { client_id }) => {
                match potential_handle.extract_stream_and_kill() {
                    None => Err(ClientPoolError::new(ErrorKind::IdentificationTimedOut)),
                    Some(client_stream) => Ok((client_id as u32, client_stream))
                }
            },
            Ok(_) => {
                potential_handle.kill();
                Err(ClientPoolError::new(ErrorKind::UnexpectedMessageType))
            }
            Err(_too_late_to_identify) => {
                potential_handle.kill();
                Err(ClientPoolError::new(ErrorKind::IdentificationTimedOut))
            }
        }
    }

    fn enable_connection(&mut self, identifier: u32, stream: TcpStream) -> Result<(), ClientPoolError> {
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
                    *client_handle.connection_state.write().unwrap() =
                        ConnectionState::Connected { controller_id: 0, stream };
                    match Self::send_to(&client_handle, Authenticated) {
                        Ok(_) => {
                            client_handle.notify_status(AliveStatus::ConnectedAndAuthenticated);
                            Ok(())
                        },
                        Err(error) => {
                            *client_handle.connection_state.write().unwrap() = ConnectionState::Disconnected;
                            Err(error)
                        }
                    }
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