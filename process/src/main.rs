use std::env::args;
use std::str::FromStr;
use log::{log_server, LogLevel};
use process::process::common::Process;

const CLIENT_COUNT: usize = 3;

fn main() {
    let mut overview: bool = false;
    let mut id: Option<u8> = None;
    let mut with_controller: bool = true;

    let mut args = args();
    args.next();
    
    for argument in args {
        match argument.as_str() {
            "--overview" => overview = true,
            "--no-controller" => with_controller = false,
            other => id = Some(extract_id(other))
        }
    }

    if !overview && id.is_none() {
        panic!("You need to specify an identifier");
    }

    if overview && id.is_some() {
        panic!("Cannot use overview with an id")
    }

    if overview && !with_controller {
        panic!("--no-controller and --overview are incompatible")
    }

    if overview {
        // Start logger server, this is blocking.
        if let Err(error) = log_server::act_as_primary_logger(LogLevel::INFO) {
            println!("Log server encountered an error: {:?}", error)
        }
    } else if with_controller {
        Process::start_with_controller(id.unwrap(), CLIENT_COUNT);
    } else {
        Process::start_without_controller(id.unwrap());
    }
}

fn extract_id(string: &str) -> u8 {
    u8::from_str(string).unwrap_or_else(|e| { panic!("{}", e) })
}