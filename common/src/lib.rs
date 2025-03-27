pub mod connection {
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
}

pub mod data_structures {
    pub mod network {
        pub mod payload;
        pub mod message;
    }

    pub mod call_request;
    pub mod cabin_state;
    pub mod call_light_array;
    pub mod full_requests_matrix;
    pub mod controller_state;
}

pub mod config;
pub mod constants;