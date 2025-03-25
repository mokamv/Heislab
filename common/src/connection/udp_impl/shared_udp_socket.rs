use libc::{bind, c_int, in6_addr, in_addr, in_port_t, sa_family_t, setsockopt, sockaddr, sockaddr_in, sockaddr_in6, socket, socklen_t, AF_INET, AF_INET6, SOCK_CLOEXEC, SOCK_DGRAM, SOL_SOCKET, SO_REUSEPORT};
use std::io;
use std::mem::zeroed;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::os::fd::FromRawFd;

// This is mostly "stolen" from rust/libc impl because they decided to make every function private
//  for some obscure reasons (I know it is not that obscure) but I need to have a special flag on my sockets.

union CAddress {
    v4: sockaddr_in,
    v6: sockaddr_in6
}

impl CAddress {
    fn to_sockaddr(&self) -> *const sockaddr {
        self as *const _ as *const sockaddr
    }
}

fn rust_addr_to_posix_addr<A: ToSocketAddrs>(addr: A) -> CAddress {
    match addr.to_socket_addrs().unwrap().next().unwrap() {
        SocketAddr::V4(socket_v4) => {
            let sockaddr4 = sockaddr_in {
                sin_family: AF_INET as sa_family_t,
                sin_port: socket_v4.port().to_be() as in_port_t,
                sin_addr: in_addr { s_addr: u32::from_ne_bytes(socket_v4.ip().octets()) },
                sin_zero: unsafe { zeroed() },
            };

            CAddress { v4: sockaddr4 }
        },
        SocketAddr::V6(socket_v6) => {
            let sockaddr6 = sockaddr_in6 {
                sin6_family: AF_INET6 as sa_family_t,
                sin6_port: socket_v6.port().to_be(),
                sin6_addr: in6_addr { s6_addr: socket_v6.ip().octets() },
                sin6_flowinfo: socket_v6.flowinfo(),
                sin6_scope_id: socket_v6.scope_id(),
                ..unsafe { zeroed() }
            };

            CAddress { v6: sockaddr6 }
        }
    }
}

pub fn udp_socket_sharing_port<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
    let c_sockaddr = rust_addr_to_posix_addr(addr);

    let udp_socket = unsafe { cvt(socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC, 0))? };

    unsafe {
        let opt_val: bool = true;
        cvt(setsockopt(
            udp_socket,
            SOL_SOCKET,
            SO_REUSEPORT,
            (&raw const opt_val) as *const _,
            size_of::<c_int>() as socklen_t,
        ))?;
    }

    unsafe {
        cvt(bind(
            udp_socket,
            c_sockaddr.to_sockaddr(),
            size_of::<sockaddr_in>() as socklen_t
        ))?;
    }


    unsafe { Ok(UdpSocket::from_raw_fd(udp_socket)) }
}

fn cvt(t: c_int) -> io::Result<c_int> {
    if t == -1 { Err(io::Error::last_os_error()) } else { Ok(t) }
}
