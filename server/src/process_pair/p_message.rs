use std::io::{stdin, Read};
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread::spawn;

type InterProcessMessage = u32;




pub(super) fn backup_read_stdin_loop() -> Receiver<InterProcessMessage> {
    let (channel_tx, channel_rx)
        = mpsc::channel::<InterProcessMessage>();

    spawn(move || {
        let mut stdin_buffer = [0u8; size_of::<u32>()]; //TODO CHANGE THIS
        let mut stdin_backup = stdin();
        loop {
            let amt = stdin_backup.read_exact(&mut stdin_buffer);
            match amt {
                // Handle error/closed pipe
                Err(_) => {
                    drop(stdin_backup);
                    drop(channel_tx);
                    break
                },

                // Normal situation, transmit message
                Ok(_) =>
                    if let Err(_send_error)
                        // Handle send error
                        = channel_tx.send(u32::from_be_bytes(stdin_buffer)) {
                        drop(stdin_backup);
                        drop(channel_tx);
                        break
                    } // TODO
            }
        }
    });

    channel_rx
}