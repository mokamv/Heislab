use common::connection::connection_handle::handle::ConnectionHandle;
use common::log::log_client::Logger;
use common::messages::{Message, TimedMessage};
use crossbeam_channel::{select, tick};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identifier = if args.len() > 1 {
        u8::from_str(args[1].as_str()).unwrap_or_else(|e| { panic!("{}", e) })
    } else {
        panic!("Need to provide an id between 0 and 255");
    };

    let faulted = Arc::new(Mutex::new(false));

    let mut logger = Logger::init(&faulted);

    let mut connection_handler = ConnectionHandle::new_client_connection_handler(
        logger.get_sender("[Client]".to_string()),
        &faulted
    );

    let message_sender = connection_handler.take_sender();
    let message_receiver = connection_handler.take_receiver();

    let mut i = 0;

    let mut is_auth = false;

    loop {
        select!(
            recv(message_receiver) -> message => {
                match message {
                    Ok(message) => {
                        match message {
                            Message::Connected => {
                                is_auth = false;
                                println!("Connected to server");
                                message_sender.send(TimedMessage::of(Message::ClientAuth { client_id: identifier })).unwrap();
                            }
                            Message::Authenticated => {
                                is_auth = true;
                                println!("Identified to the server, starting online mode")
                            }

                            Message::Disconnected => {
                                is_auth = false;
                                println!("Disconnected from server, starting offline mode");
                            }

                            _ => {}
                        }
                    }
                    Err(_) => { todo!() }
                }
            },

            recv(tick(Duration::from_millis(10))) -> _ => {
                if is_auth {
                    message_sender.send(TimedMessage::of(Message::GotoFloor {go_to_floor: i})).unwrap();
                    i = i.wrapping_add(1);
                }
            },
        );
    }
}
