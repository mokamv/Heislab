use std::env::args;
use std::str::FromStr;
use common::connection::event_handle::controller_handle::controller_handle::ControllerHandleConfiguration;
use common::connection::event_handle::handle_pool::HandlePool;
use common::connection::receiver::epoll::epoll_receiver::EpollReceiver;
use log::{log_server, LogLevel};
use log::log_client::Logger;
use process::elevator::client::standalone_process::start_standalone_process_thread;
use process::elevator::controller::controller_process::start_controller_process_thread;

const CLIENT_COUNT: usize = 3; // TODO MOVE TO CONFIG

fn main() {
    let mut overview: bool = false;
    let mut id: Option<u8> = None;
    let mut with_controller: bool = true;
    let mut with_client: bool = true;

    let mut args = args();
    args.next();
    
    for argument in args {
        match argument.as_str() {
            "--overview" => overview = true,
            "--no-controller" => with_controller = false,
            "--no-client" => with_client = false,
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

    if overview && !with_client {
        panic!("--no-client and --overview are incompatible")
    }

    if ! (with_client || with_controller) {
        panic!("--no-client and --no-controller are incompatible")
    }

    if overview {
        // Start log server, this is blocking.
        if let Err(error) = log_server::act_as_primary_logger(LogLevel::INFO) {
            println!("Log server encountered an error: {:?}", error)
        }
    } else {
        start_process(id.unwrap(), with_client, with_controller);
    }
}

fn extract_id(string: &str) -> u8 {
    u8::from_str(string).unwrap_or_else(|e| { panic!("{}", e) })
}

fn start_process(
    process_id: u8,
    standalone_enabled: bool,
    controller_enabled: bool
) {
    assert!(
        standalone_enabled || controller_enabled,
        "The program needs to start at least one of the components."
    );

    Logger::init_logger();
    Logger::send_once(format!("[{}][MAIN] Server has started in backup mode", process_id), LogLevel::INFO);

    let (mut epoll_receiver, udp_socket) = EpollReceiver::init();
    let bind_address = udp_socket.local_addr().unwrap();

    println!("RECEIVER BOUND TO: {bind_address}");

    let mut handle_pool = HandlePool::init(udp_socket);

    let standalone_handle = if standalone_enabled {
        Some(handle_pool.with_standalone_handle(&mut epoll_receiver, process_id))
    } else { None };

    let controller_handle = if controller_enabled {
        let mut controller_config =
            ControllerHandleConfiguration::new(process_id);

        controller_config
            .add_client(0)
            .add_client(1)
            .add_client(2);

        Some(
            handle_pool.with_controller_handle(
                &mut epoll_receiver,
                bind_address,
                controller_config
            )
        )
    } else { None };

    let epoll_thread = epoll_receiver.start_receiver_thread_from_builder();
    let pool_thread = handle_pool.start_handle_pool_thread();
    let standalone_thread = start_standalone_process_thread(standalone_handle);
    let controller_thread = start_controller_process_thread(
        controller_handle,
        process_id
    );

    let _ = standalone_thread.join().unwrap();
    let _ = controller_thread.join().unwrap();
    let _ = epoll_thread.join().unwrap();
    let _ = pool_thread.join().unwrap();

    Logger::terminate_logging();
}