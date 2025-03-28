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
    /// Create a new [DoorControl] instance with the given poll period.
    pub(in super) fn new(poll_period: Duration) -> DoorControl {
        let (close_door_tx, close_door_rx) = unbounded::<()>();
        
        let is_obstructed = Arc::new(AtomicBool::new(false));
        let is_open = Arc::new(AtomicBool::new(false));

        let door_control = DoorControl {
            close_door_receiver: close_door_rx,
            is_obstructed: is_obstructed.clone(),
            is_open: is_open.clone()
        };

        // Spawn a new thread to handle the door control
        spawn(move || {
            loop {
                // If the door is open, check if it should be closed
                if is_open.load(Relaxed) {
                    let mut begin = Instant::now(); // Start the timer
                    // Loop until the door is obstructed or the door has been open for too long
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

                    // Close the door when the timer is up and send the close door event
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