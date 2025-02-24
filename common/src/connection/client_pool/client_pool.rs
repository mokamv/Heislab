use crate::connection::client_pool::connection_error::{ClientPoolError, ErrorKind};
use crate::connection::connection_handle::ConnectionHandle;
use crate::messages::{Message, RawMessage};
use std::ops::DerefMut;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::spawn;
use std::time::Duration;
use crate::log::log_client::ReliableLogSender;
use crate::program_fault::{program_set_to_faulted, Faulted};

const IDENTIFICATION_TIMEOUT: Duration = Duration::from_millis(5000);

pub enum Target {
    All,
    Specific(u32)
}

pub struct ClientMessage {
    pub identifier: u32,
    pub message: Message
}

struct Client {
    identifier: u32,
    connection: Arc<Mutex<Option<ConnectionHandle>>>
}

impl Client {
    fn setup_listen_to_status_loop(&mut self, logger: &ReliableLogSender, faulted: &Faulted) {
        if let Some(connection) = self.connection.lock().unwrap().deref_mut() {

            let status = connection.take_status();
            let logger = logger.clone(); //TODO LOG
            let faulted = faulted.clone();
            let connection = self.connection.clone();

            spawn(move || {
                while connection.lock().unwrap().is_some() {
                    if *faulted.lock().unwrap() { break }
                    match status.recv() {
                        Ok(is_connected) => if !is_connected {
                            Self::kill_connection(connection);
                            break
                        },
                        Err(_) => {
                            Self::kill_connection(connection);
                            break
                        }
                    }
                }
            });
        } else {
            program_set_to_faulted(&faulted, "Cannot call this function on dead connection")
        }
    }

    fn setup_message_decode_loop(&mut self, redirect_to: &Sender<ClientMessage>, logger: &ReliableLogSender, faulted: &Faulted) {
        if let Some(connection) = self.connection.lock().unwrap().deref_mut() {

            let raw_messages_queue = connection.take_receiver();
            let identifier = self.identifier;
            let redirect_to = redirect_to.clone();
            let faulted = faulted.clone();
            let logger = logger.clone(); //TODO CLONE
            let connection = self.connection.clone();

            spawn(move || {
                while connection.lock().unwrap().is_some() {
                    if *faulted.lock().unwrap() { break }

                    match raw_messages_queue.recv() {
                        Ok(raw_message) => {
                            if let Err(_main_channel_severed) = redirect_to.send(ClientMessage {
                                identifier,
                                message: Message::decode_message(&raw_message)
                            }) {
                                // Handle general crash of the socket.
                                Self::kill_connection(connection);
                                *faulted.lock().unwrap() = true;
                                break;
                            }
                        },
                        Err(_connection_channel_severed) => {
                            Self::kill_connection(connection);
                            break
                        }
                    }
                }
            });
        } else {
            program_set_to_faulted(&faulted, "Cannot call this function on dead connection")
        }
    }

    fn kill_connection(connection: Arc<Mutex<Option<ConnectionHandle>>>) {
        let mut connection = connection.lock().unwrap();
        if let Some(connection) = connection.take() {
            connection.kill();
        }
        *connection = None;
    }
}

#[derive(Clone)]
pub struct ClientPool {
    shared_pool: Arc<Mutex<ClientPoolShared>>
}

impl ClientPool {
    pub fn init(faulted: &Faulted, logger: ReliableLogSender, max_client_nb: usize) -> Self {
        Self {
            shared_pool: Arc::new(Mutex::new(ClientPoolShared::init(faulted, logger, max_client_nb))),
        }
    }

    pub fn with_client_id(&mut self, client_id: u32) -> &mut Self {
        self.shared_pool.lock().unwrap().with_client_id(client_id);
        self
    }

    pub fn handle_connection(&mut self, connection: ConnectionHandle) -> Result<(), ClientPoolError> {
        self.shared_pool.lock().unwrap().wait_for_identification(connection)
    }

    pub fn send(&mut self, target: Target, message: Message) -> Result<(), ClientPoolError> {
        self.shared_pool.lock().unwrap().send(target, message)
    }

    pub fn get_message_channel(&mut self) -> Receiver<ClientMessage> {
        self.shared_pool.lock().unwrap().receiver.take().unwrap()
    }
}

struct ClientPoolShared {
    max_client_nb: usize,
    clients: Vec<Client>,
    logger: ReliableLogSender,
    faulted: Faulted,
    receiver: Option<Receiver<ClientMessage>>,
    sender: Sender<ClientMessage>
}

impl ClientPoolShared {
    fn init(faulted: &Faulted, logger: ReliableLogSender, managed_clients: usize) -> Self {
        let (tx, rx) = channel();

        Self {
            max_client_nb: managed_clients,
            clients: Vec::with_capacity(managed_clients),
            faulted: faulted.clone(),
            logger,
            receiver: Some(rx),
            sender: tx,
        }
    }

    fn with_client_id(&mut self, client_id: u32) {
        if self.clients.len() >= self.max_client_nb {
            program_set_to_faulted(&self.faulted, "You cannot add more clients")
        }

        self.clients.push(
            Client {
                identifier: client_id,
                connection: Arc::new(Mutex::new(None)),
            }
        );
    }

    fn send(&mut self, target: Target, message: Message) -> Result<(), ClientPoolError> {
        let raw_message = message.encode();

        match target {
            Target::All => {
                let mut all_failed = true;
                for target in &self.clients {
                    let mut connection = target.connection.lock().unwrap();
                    if let Some(target_handle) = connection.take() {
                        all_failed = false;

                        if let Err(_) = Self::send_to(&target_handle, raw_message) {
                            target_handle.kill();
                        } else {
                            *connection = Some(target_handle);
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
                        let mut connection = client.connection.lock().unwrap();
                        match connection.take() {
                            None => Err(ClientPoolError::new(ErrorKind::ClientIsDisconnected)),
                            Some(target_handle) => {
                                if let Err(_) = Self::send_to(&target_handle, raw_message) {
                                    target_handle.kill();
                                    Ok(())
                                } else {
                                    *connection = Some(target_handle);
                                    Ok(())
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn wait_for_identification(&mut self, potential_client: ConnectionHandle) -> Result<(), ClientPoolError> {
        let message_receiver = potential_client.borrow_receiver();
        let (identifier, client) = match message_receiver.recv_timeout(IDENTIFICATION_TIMEOUT) {
            Ok(raw_message) => {
                if let Message::ClientAuth { client_id } = Message::decode_message(&raw_message) {
                    Ok((client_id as u32, potential_client))
                } else {
                    potential_client.kill();
                    Err(ClientPoolError::new(ErrorKind::UnexpectedMessageType))
                }
            }
            Err(_too_late_to_identify) => {
                potential_client.kill();
                Err(ClientPoolError::new(ErrorKind::IdentificationTimedOut))
            }
        }?;

        self.enable_connection(identifier, client)
    }

    fn enable_connection(&mut self, identifier: u32, handle: ConnectionHandle) -> Result<(), ClientPoolError> {
        match self.clients.iter_mut().find(|x| {
            x.identifier == identifier
        }) {
            None => {
                handle.kill();
                Err(ClientPoolError::new(ErrorKind::BadIdentifier))
            },
            Some(client) => {
                // Check for already connected instance, this is an error
                if client.connection.lock().unwrap().is_some() {
                    handle.kill();
                    Err(ClientPoolError::new(ErrorKind::AlreadyConnected))
                } else {
                    *client.connection.lock().unwrap() = Some(handle);
                    client.setup_listen_to_status_loop(&self.logger, &self.faulted);
                    client.setup_message_decode_loop(&self.sender, &self.logger, &self.faulted);
                    Ok(())
                }
            }
        }
    }

    fn send_to(target_handle: &ConnectionHandle, message: RawMessage) -> Result<(), ClientPoolError> {
        if let Err(_connection_channel_severed) = target_handle.borrow_sender().send(message) {
            return Err(ClientPoolError::new(ErrorKind::DeadHandle))
        };
        Ok(())
    }
}