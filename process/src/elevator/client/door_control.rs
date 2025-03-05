use crossbeam_channel::{unbounded, Receiver};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

const STAYS_OPEN_FOR: Duration = Duration::from_secs(3);
const POLL_TIME: Duration = Duration::from_millis(25);

pub struct DoorControl {
    is_obstructed: Arc<AtomicBool>,
    is_open: Arc<AtomicBool>,
}

impl DoorControl {
    pub fn new(poll_period: Duration) -> (DoorControl, Receiver<()>) {

        let (close_door_tx, close_door_rx) = unbounded::<()>();


        let is_obstructed = Arc::new(AtomicBool::new(false));
        let is_open = Arc::new(AtomicBool::new(false));

        let door_control = DoorControl {
            is_obstructed: is_obstructed.clone(),
            is_open: is_open.clone()
        };

        spawn(move || {
            loop {
                if is_open.load(Relaxed) {
                    let mut begin = Instant::now();
                    'timer: loop {
                        if is_obstructed.load(Relaxed) {
                            begin = Instant::now();
                            continue 'timer
                        }

                        if begin.elapsed() > STAYS_OPEN_FOR {
                            break 'timer
                        }
                    }

                    close_door_tx.send(()).expect("Unexpected state");
                    is_open.store(false, Relaxed);
                }
                sleep(poll_period);
            }
        });

        (door_control, close_door_rx)
    }

    pub fn open_door(&self) {
        self.is_open.store(true, Relaxed);
    }

    pub fn obstruction(&self, obstructed: bool) {
        self.is_obstructed.store(obstructed, Relaxed);
    }
}

#[cfg(test)]
mod door_tests {
    use super::*;

    use crossbeam_channel::{select, Receiver};
    use std::sync::{Arc, Mutex};
    use std::thread::{sleep, spawn};
    use std::time::{Duration, Instant};

    const POLL_DURATION: Duration = Duration::from_millis(25);

    fn listen_to_close_event(close_rx: Receiver<()>, closed_at: Arc<Mutex<Instant>>) {
        spawn(move || {
            'thread_loop: loop {
                select! {
                    recv(close_rx) -> _ => {
                        { *closed_at.lock().unwrap() = Instant::now(); }
                        break 'thread_loop;
                    }
                }
            }
        });
    }

    #[test]
    fn door_close_timer() {
        let (door_control, close_rx) = DoorControl::new(POLL_DURATION);
        let closed_at = Arc::new(Mutex::new(Instant::now()));
        listen_to_close_event(close_rx, closed_at.clone());

        let opened_at = Instant::now();
        door_control.open_door();
        sleep(Duration::from_secs(5));

        assert!(closed_at.lock().unwrap().duration_since(opened_at) > STAYS_OPEN_FOR);
        assert_eq!(door_control.is_open.load(Relaxed), false);
        assert_eq!(door_control.is_obstructed.load(Relaxed), false);
    }

    #[test]
    fn door_obstructed() {
        let (door_control, close_rx) = DoorControl::new(POLL_DURATION);
        let closed_at = Arc::new(Mutex::new(Instant::now()));
        listen_to_close_event(close_rx, closed_at.clone());

        let opened_at = Instant::now();
        door_control.open_door();
        door_control.obstruction(true);

        let mut i = 0;
        while i < 5 {
            sleep(Duration::from_secs(2));
            assert_eq!(door_control.is_open.load(Relaxed), true);
            assert_eq!(door_control.is_obstructed.load(Relaxed), true);
            i += 1;
        }

        door_control.obstruction(false);
        sleep(Duration::from_secs(4));

        assert!(closed_at.lock().unwrap().duration_since(opened_at) > Duration::from_secs(10) + STAYS_OPEN_FOR);
        assert_eq!(door_control.is_open.load(Relaxed), false);
        assert_eq!(door_control.is_obstructed.load(Relaxed), false);
    }
}