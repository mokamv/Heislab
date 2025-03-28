use std::env::args;
use std::str::FromStr;
use common::config::VALID_CLIENT_IDS;
use common::connection::event_handle::controller_handle::controller_handle::ControllerHandleConfiguration;
use common::connection::event_handle::handle_pool::HandlePool;
use common::connection::event_handle::handle_state::ConnectionIdentifier;
use common::connection::receiver::epoll::epoll_receiver::EpollReceiver;
use process::elevator::client::standalone_process::start_standalone_process_thread;
use process::elevator::controller::controller_process::start_controller_process_thread;

fn main() {
    // Parse command line arguments
    let mut id: Option<ConnectionIdentifier> = None;
    let mut with_controller: bool = true;
    let mut with_client: bool = true;

    let mut args = args();
    args.next();
    
    for argument in args {
        match argument.as_str() {
            "--no-controller" => with_controller = false,
            "--no-client" => with_client = false,
            other => id = Some(extract_id(other))
        }
    }

    if id.is_none() {
        panic!("You need to specify an identifier");
    }

    if ! (with_client || with_controller) {
        panic!("--no-client and --no-controller are incompatible")
    }
    start_process(id.unwrap(), with_client, with_controller);
}

fn extract_id(string: &str) -> ConnectionIdentifier {
    let potential_id = ConnectionIdentifier::from_str(string).unwrap_or_else(|e| { panic!("{}", e) });
    if !VALID_CLIENT_IDS.contains(&potential_id) {
        panic!("{potential_id} is not a valid id, please use of those: {VALID_CLIENT_IDS:?}")
    } else {
        potential_id
    }
}

/// Start the process with the given id and the given components enabled.
fn start_process(
    process_id: u8,
    standalone_enabled: bool,
    controller_enabled: bool
) {
    // Check if at least one of the components is enabled
    assert!(
        standalone_enabled || controller_enabled,
        "The program needs to start at least one of the components."
    );

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

        for valid_client_id in VALID_CLIENT_IDS {
            controller_config.add_client(valid_client_id);
        }

        Some(
            handle_pool.with_controller_handle(
                &mut epoll_receiver,
                bind_address,
                controller_config
            )
        )
    } else { None };

    // Start the threads
    let epoll_thread = epoll_receiver.start_receiver_thread_from_builder();
    let pool_thread = handle_pool.start_handle_pool_thread();
    let standalone_thread = start_standalone_process_thread(standalone_handle);
    let controller_thread = start_controller_process_thread(
        controller_handle,
        process_id
    );

    // Wait for the threads to finish
    let _ = standalone_thread.join().unwrap();
    let _ = controller_thread.join().unwrap();
    let _ = epoll_thread.join().unwrap();
    let _ = pool_thread.join().unwrap();
}