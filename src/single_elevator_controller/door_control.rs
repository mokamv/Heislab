use crossbeam_channel as cbc;
use crossbeam_channel::{select, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

const STAYS_OPEN_FOR: Duration = Duration::from_secs(3);
const POLL_TIME: Duration = Duration::from_millis(25);

pub struct DoorControl {
    state: DoorState,
    obstruction_tx: Sender<bool>,
    open_door_tx: Sender<()>,
}

#[derive(Clone)]
struct DoorState {
    is_obstructed: Arc<Mutex<bool>>,
    is_open: Arc<Mutex<bool>>,
}

impl DoorControl {
    pub fn new() -> (DoorControl, Receiver<()>) {

        let (close_door_tx, close_door_rx) = cbc::unbounded::<()>();
        let (open_door_tx, open_door_rx) = cbc::unbounded::<()>();
        let (obstruction_tx, obstruction_rx) = cbc::unbounded::<bool>();

        let door_control = DoorControl {
            state: DoorState {
                is_obstructed: Arc::new(Mutex::new(false)),
                is_open: Arc::new(Mutex::new(false))
            },
            obstruction_tx,
            open_door_tx,
        };

        {
            let state = door_control.state.clone();
            spawn(move || close_counter(state, close_door_tx, POLL_TIME));
        }

        {
            let state = door_control.state.clone();
            spawn(move || {
                loop {
                    select! {
                        recv(open_door_rx) -> _ =>
                            *state.is_open.lock().unwrap() = true,
                        recv(obstruction_rx) -> is_obstructed =>
                            *state.is_obstructed.lock().unwrap() = is_obstructed.unwrap(),
                    }
                }
            });
        }

        (door_control, close_door_rx)
    }

    pub fn open(&self) {
        self.open_door_tx.send(()).expect("Unexpected state");
    }

    pub fn obstruction(&self, obstructed: bool) {
        self.obstruction_tx.send(obstructed).expect("Unexpected state");
    }
}

fn close_counter(state: DoorState, close_door_tx: Sender<()>, poll_period: Duration) {
    loop {
        sleep(poll_period);
        let is_open = { *state.is_open.lock().unwrap() };

        if is_open == true {
            let mut begin = Instant::now();
            'timer: loop {
                {
                    let is_obstructed = { *state.is_obstructed.lock().unwrap() };
                    if is_obstructed == true {
                        begin = Instant::now();
                        continue 'timer
                    }
                }

                if Instant::now().duration_since(begin) > STAYS_OPEN_FOR {
                    break 'timer
                }
            }
            *state.is_open.lock().unwrap() = false;
            close_door_tx.send(()).expect("Unexpected state");
        }
    }
}

#[cfg(test)]
mod door_tests {
    use super::*;

    use std::sync::{Arc, Mutex};
    use std::thread::{sleep, spawn};
    use std::time::{Duration, Instant};
    use crossbeam_channel::{select, Receiver};

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
        let (door_control, close_rx) = DoorControl::new();
        let closed_at = Arc::new(Mutex::new(Instant::now()));
        listen_to_close_event(close_rx, closed_at.clone());

        let opened_at = Instant::now();
        door_control.open();
        sleep(Duration::from_secs(5));

        assert!(closed_at.lock().unwrap().duration_since(opened_at) > STAYS_OPEN_FOR);
        assert_eq!(*door_control.state.is_open.lock().unwrap(), false);
        assert_eq!(*door_control.state.is_obstructed.lock().unwrap(), false);
    }

    #[test]
    fn door_obstructed() {
        let (door_control, close_rx) = DoorControl::new();
        let closed_at = Arc::new(Mutex::new(Instant::now()));
        listen_to_close_event(close_rx, closed_at.clone());

        let opened_at = Instant::now();
        door_control.open();
        door_control.obstruction(true);

        let mut i = 0;
        while i < 5 {
            sleep(Duration::from_secs(2));
            assert_eq!(*door_control.state.is_open.lock().unwrap(), true);
            assert_eq!(*door_control.state.is_obstructed.lock().unwrap(), true);
            i += 1;
        }

        door_control.obstruction(false);
        sleep(Duration::from_secs(4));

        assert!(closed_at.lock().unwrap().duration_since(opened_at) > Duration::from_secs(10) + STAYS_OPEN_FOR);
        assert_eq!(*door_control.state.is_open.lock().unwrap(), false);
        assert_eq!(*door_control.state.is_obstructed.lock().unwrap(), false);
    }
}