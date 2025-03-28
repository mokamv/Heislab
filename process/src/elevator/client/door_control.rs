use crossbeam_channel::{unbounded, Receiver};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};
use common::config::DOOR_OPEN_DURATION;

pub struct DoorControl {
    close_door_receiver: Receiver<()>,
    is_obstructed: Arc<AtomicBool>,
    is_open: Arc<AtomicBool>,
}

impl DoorControl {
    pub(in super) fn new(poll_period: Duration) -> DoorControl {
        let (close_door_tx, close_door_rx) = unbounded::<()>();
        
        let is_obstructed = Arc::new(AtomicBool::new(false));
        let is_open = Arc::new(AtomicBool::new(false));

        let door_control = DoorControl {
            close_door_receiver: close_door_rx,
            is_obstructed: is_obstructed.clone(),
            is_open: is_open.clone()
        };

        spawn(move || {
            loop {
                if is_open.load(Relaxed) {
                    let mut begin = Instant::now();
                    'timer: loop {
                        sleep(poll_period);
                        if is_obstructed.load(Relaxed) {
                            begin = Instant::now();
                            continue 'timer
                        }

                        if begin.elapsed() > DOOR_OPEN_DURATION {
                            break 'timer
                        }
                    }

                    close_door_tx.send(()).expect("Unexpected state");
                    is_open.store(false, Relaxed);
                }
                sleep(poll_period);
            }
        });

        door_control
    }

    pub(in super) fn recv_closed_door_event(&self) -> &Receiver<()> {
        &self.close_door_receiver
    }

    pub(in super) fn open_door(&self) {
        self.is_open.store(true, Relaxed);
    }

    pub(in super) fn update_obstruction(&self, obstructed: bool) {
        self.is_obstructed.store(obstructed, Relaxed);
    }

    pub(in super) fn is_obstructed(&self) -> bool {
        self.is_obstructed.load(Relaxed)
    }
}

// #[cfg(test)]
// mod door_tests {
//     use super::*;
//
//     use crossbeam_channel::{select, Receiver};
//     use std::sync::{Arc, Mutex};
//     use std::thread::spawn;
//     use std::time::{Duration, Instant};
//
//     const POLL_DURATION: Duration = Duration::from_millis(25);
//
//     fn listen_to_close_event(close_rx: &Receiver<()>, closed_at: Arc<Mutex<Instant>>) {
//         let close_rx = close_rx.clone();
//         spawn(move || {
//             'thread_loop: loop {
//                 select! {
//                     recv(close_rx) -> _ => {
//                         { *closed_at.lock().unwrap() = Instant::now(); }
//                         break 'thread_loop;
//                     }
//                 }
//             }
//         });
//     }
//
//     // #[test]
//     // fn door_close_timer() {
//     //     let door_control = DoorControl::new(POLL_DURATION);
//     //     let closed_at = Arc::new(Mutex::new(Instant::now()));
//     //     listen_to_close_event(door_control.recv_closed_door_event(), closed_at.clone());
//     //
//     //     let opened_at = Instant::now();
//     //     door_control.open_door();
//     //     sleep(Duration::from_secs(5));
//     //
//     //     assert!(closed_at.lock().unwrap().duration_since(opened_at) > STAYS_OPEN_FOR);
//     //     assert_eq!(door_control.is_open.load(Relaxed), false);
//     //     assert_eq!(door_control.is_obstructed.load(Relaxed), false);
//     // }
//     //
//     // #[test]
//     // fn door_obstructed() {
//     //     let door_control= DoorControl::new(POLL_DURATION);
//     //     let closed_at = Arc::new(Mutex::new(Instant::now()));
//     //     listen_to_close_event(door_control.recv_closed_door_event(), closed_at.clone());
//     //
//     //     let opened_at = Instant::now();
//     //     door_control.open_door();
//     //     door_control.update_obstruction(true);
//     //
//     //     let mut i = 0;
//     //     while i < 5 {
//     //         sleep(Duration::from_secs(2));
//     //         assert_eq!(door_control.is_open.load(Relaxed), true);
//     //         assert_eq!(door_control.is_obstructed.load(Relaxed), true);
//     //         i += 1;
//     //     }
//     //
//     //     door_control.update_obstruction(false);
//     //     sleep(Duration::from_secs(4));
//     //
//     //     assert!(closed_at.lock().unwrap().duration_since(opened_at) > Duration::from_secs(10) + STAYS_OPEN_FOR);
//     //     assert_eq!(door_control.is_open.load(Relaxed), false);
//     //     assert_eq!(door_control.is_obstructed.load(Relaxed), false);
//     // }
// }