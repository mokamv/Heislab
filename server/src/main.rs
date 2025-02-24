use server::ProcessPair;
use std::env::args;
use std::str::FromStr;
use common::log::LogLevel;

fn main() {
    let args: Vec<String> = args().collect();
    
    if args.len() > 1 && args[1] == "--backup" {
        ProcessPair::run_as_backup(
            if args.len() > 2 {
                u32::from_str(args[2].as_str()).unwrap_or(0)
            } else {
                0
            }
        )
    } else if args.len() > 1 && args[1] == "--main" {
        ProcessPair::run_as_main()
    } else {
        //TODO FROM EXEC PARAM.
        ProcessPair::run_as_overview(LogLevel::DEBUG)
    };
}