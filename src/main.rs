use std::env;
use std::env::current_exe;
use std::net::UdpSocket;
use std::process::Command;
use std::str::FromStr;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Duration;

const COUNTER_PERIOD: Duration = Duration::from_millis(200);
const ERROR_RATE: f64 = 0.05;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--backup" {
        Process::start_backup(if args.len() > 1 { u32::from_str(args[2].as_str()).unwrap_or(0) } else {0} );
    } else {
        Process::start_main()
    }
}

struct Process;

impl Process {
    fn start_main() {
        Self::launch_child(0);
        BackedUpCounter::new().start_counting();
    }

    fn create_udp_broadcaster() -> UdpSocket {
        let udp = UdpSocket::bind("0.0.0.0:0").unwrap();
        udp.set_broadcast(true).expect("TODO: panic message");
        udp
    }

    fn is_wsl() -> bool {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/c").arg("exit");

        match cmd
            .status() {
            Err(_) => {
                false
            },
            Ok(status) => {
                match status.code() {
                    None => false,
                    Some(code) => code == 0
                }
            }
        }
    }

    fn launch_in_new_wsl_terminal(command: String) {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/c");
        cmd.arg("start");
        cmd.arg("bash");
        cmd.arg("-c");
        cmd.arg(command);

        match cmd
            .spawn() {
            Err(why) => panic!("couldn't spawn backup: {}", why),
            Ok(_) => {}
        };
    }

    fn launch_in_new_terminal(command: String) {
        let mut cmd = Command::new("gnome-terminal");
        cmd.arg("--");
        cmd.arg(command);

        match cmd
            .spawn() {
            Err(why) => panic!("couldn't spawn backup: {}", why),
            Ok(_) => {}
        };
    }

    fn launch_child(current_count: u32) {
        #[cfg(target_os = "linux")] {
            if Self::is_wsl() {
                Self::launch_in_new_wsl_terminal(format!("{:?} --backup {}", current_exe().unwrap(), current_count));
            } else {
                Self::launch_in_new_terminal(format!("{:?} --backup {}", current_exe().unwrap(), current_count));
            }
        }
    }

    fn start_backup(current_count: u32) {
        BackedUpCounter::listen_to_master(current_count).takeover_master();
    }
}

struct BackedUpCounter {
    count: Arc<Mutex<u32>>,
}

impl BackedUpCounter {
    fn new() -> BackedUpCounter {
        let counter = BackedUpCounter { count: Arc::new(Mutex::new(0)) };
        counter
    }

    fn start_counting(self) {
        let intern_count = self.count.clone();
        let backup_to = Process::create_udp_broadcaster();
        loop {
            let is_error = rand::random_bool(ERROR_RATE);
            if is_error {
                println!("Main is crashing");
                break
            }

            // increment then get a copy (to let free of the mutex)
            let current_count = {
                let mut current_count = intern_count.lock().unwrap();
                *current_count += 1;
                current_count.clone()
            };

            // Write to both stdout and the backup
            println!("{}", current_count);
            match backup_to.send_to(current_count.to_be_bytes().as_slice(), "255.255.255.255:10000") {
                Ok(_) => {}
                Err(_) => {}
            };

            sleep(COUNTER_PERIOD);
        };
    }

    fn takeover_master(self) {
        Process::launch_child(*self.count.lock().unwrap());
        self.start_counting();
    }

    fn listen_to_master(current_count: u32) -> Self {
        let counter = BackedUpCounter { count: Arc::new(Mutex::new(current_count)) };

        let intern_count = counter.count.clone();

        let stdin_channel = Self::spawn_udp_channel();

        let mut time_since_last_response = 0;
        loop {
            match stdin_channel.try_recv() {
                Ok(read_count) => {
                    if read_count == u32::MAX {
                        println!("Main informed that it crashed");
                        break
                    }

                    let mut current_count = intern_count.lock().unwrap();
                    if read_count > *current_count {
                        time_since_last_response = 0;
                        *current_count = read_count;
                    }
                }
                Err(TryRecvError::Empty) => {
                    time_since_last_response += 1;

                    if time_since_last_response > 10 {
                        println!("Main is unresponsive");
                        break
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    break
                },
            }

            sleep(COUNTER_PERIOD);
        }

        counter
    }

fn spawn_udp_channel() -> Receiver<u32> {
    let socket = UdpSocket::bind("0.0.0.0:10000").unwrap();

        let (tx, rx) = mpsc::channel::<u32>();
        spawn(move || loop {
            let mut buffer = [0u8; size_of::<u32>()];
            let (_amt, _src) = socket.recv_from(&mut buffer).unwrap();
            let value = u32::from_be_bytes(buffer);
            match tx.send(value) {
                Ok(_) => {}
                Err(_) => {break}
            };
            if value == u32::MAX {
                break;
            }
        });
        rx
    }
}