pub mod udp_impl {
    pub mod udp_ack_socket;
    pub mod udp_read;
    pub mod shared_udp_socket;
}

pub mod receiver {
    pub mod epoll {
        pub mod receiver_type;
        pub mod epoll;
        pub mod epoll_receiver;
    }

    pub mod wrapper {
        pub mod udp_receiver;
        pub mod udp_broadcast_receiver;
    }
}

pub mod event_handle {
    pub mod handle_state;
    pub mod handle_pool;
    pub mod standalone_handle {
        pub mod standalone_handle;
        pub mod standalone_handle_epoll_channels;
        pub mod standalone_handle_pool_channels;
    }
    pub mod controller_handle {
        pub mod controller_handle;
        pub mod controller_handle_epoll_channels;
        pub mod controller_handle_pool_channels;
    }
}

mod constants { //TODO MOVE TO A CONFIG FILE
    use std::time::Duration;

    pub const UDP_TIMEOUT: Duration = Duration::from_millis(750);
    pub const UDP_RETRY: Duration = Duration::from_millis(5);
    pub const SEND_KEEP_ALIVE_PERIOD: Duration = Duration::from_millis(25);
    pub const BROADCAST_PERIOD: Duration = Duration::from_millis(250);

    pub mod ip_addresses {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        pub const CONTROLLER_UDP_BIND_PORT: u16 = 9000;
        pub const CONTROLLER_UDP_BIND_ADDR: SocketAddr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), CONTROLLER_UDP_BIND_PORT);

        pub const COMMON_UDP_BIND_ADDR: SocketAddr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);

        pub const SERVER_UDP_BROADCAST_PORT: u16 = 9001;
        pub const SERVER_UDP_BROADCAST_ADDR: SocketAddr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), SERVER_UDP_BROADCAST_PORT);
        pub const HANDLE_UDP_LISTEN_ADDR: SocketAddr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), SERVER_UDP_BROADCAST_PORT);
    }
}

