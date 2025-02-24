use common::connection::connection_handle::ConnectionHandle;
use common::messages::Message;
use std::sync::{Arc, Mutex};
use std::thread::spawn;
use common::log::log_client::Logger;

fn main() {
    println!("Hello, world!");

    let faulted = Arc::new(Mutex::new(false));

    let mut logger = Logger::init(&faulted);

    let mut connection_handler = ConnectionHandle::new_client_connection_handler(
        logger.get_sender("[Client]".to_string()),
        &faulted
    );


    let message_sender = connection_handler.take_sender();
    let message_channel = connection_handler.take_receiver();
    let connected = connection_handler.take_status();

    spawn(move ||{
        loop {
            match message_channel.recv() {
                Ok(message) => {
                    print!("{}", String::from_utf8_lossy(&message))
                }
                Err(_) => {break}
            }
        }
    } );

    loop {
        match connected.recv() {
            Ok(connected) => {
                match connected {
                    true => {
                        message_sender.send(Message::encode(Message::ClientAuth { client_id: 1 })).unwrap();
                        println!("Connected to server");
                    },
                    false => println!("Disconnected from server")
                }
            }
            Err(_) => {break}
        }
    }

}
