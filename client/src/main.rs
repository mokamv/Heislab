use common::connection::connection_handle::ConnectionHandle;
use common::log::log_client::Logger;
use common::messages::Message;
use crossbeam_channel::{select, tick};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use common::connection::connection_channel::AliveStatus;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let identifier = if args.len() > 1 {
        u8::from_str(args[1].as_str()).unwrap_or_else(|e| { panic!("{}", e) })
    } else {
        panic!("Need to provide an id between 0 and 255");
    };


    println!("Hello, world!");

    let faulted = Arc::new(Mutex::new(false));

    let mut logger = Logger::init(&faulted);

    let mut connection_handler = ConnectionHandle::new_client_connection_handler(
        logger.get_sender("[Client]".to_string()),
        &faulted
    );


    let message_sender = connection_handler.take_sender();
    let message_receiver = connection_handler.take_receiver();
    let connected = connection_handler.take_status();

    let mut i = 0;
    loop {
        select!(
            recv(message_receiver) -> message => {
                match message {
                    Ok(message) => {
                        print!("{:?}", message)
                    }
                    Err(_) => {break}
                }
            },
            recv(connected) -> status => {
                match status {
                    Ok(connected) => {
                        match connected {
                            AliveStatus::Connected => {
                                println!("Connected to server");
                                message_sender.send(Message::ClientAuth { client_id: identifier }).unwrap();
                            }
                            AliveStatus::ConnectedAndAuthenticated => {
                                println!("Identified to the server, starting online mode")
                            }

                            AliveStatus::Disconnected => {
                                println!("Disconnected from server, starting offline mode");
                            }
                        }
                    }
                    Err(_) => {}
                }
            },
            recv(tick(Duration::from_secs(1))) -> instant => {
                message_sender.send(Message::GotoFloor {go_to_floor: i}).unwrap();
                i = i.wrapping_add(1);
            },
        );
    }
}
