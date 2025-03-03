use server::ProcessPair;
use std::env::args;
use std::str::FromStr;
use log::LogLevel;

const CLIENT_COUNT: usize = 3;

fn main() {
    let args: Vec<String> = args().collect();
    
    if args.len() > 1 && args[1] == "--overview" {
        ProcessPair::run_as_overview(LogLevel::DEBUG)
    } else {
        ProcessPair::run_as_program(
            if args.len() > 1 {
                u8::from_str(args[1].as_str()).unwrap_or_else(|e| { panic!("{}", e) })
            } else {
                panic!("Need to provide an id between 0 and 255");
            }, CLIENT_COUNT
        )
    };
}