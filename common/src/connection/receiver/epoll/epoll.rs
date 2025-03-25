use libc::{c_int, epoll_event, EPOLLIN, EPOLLONESHOT};
use std::io;
use std::os::fd::RawFd;

const STANDARD_CAPACITY: c_int = 1024;
const READ_FLAGS: c_int = EPOLLONESHOT | EPOLLIN;
const EPOLL_TIMEOUT_MS: c_int = 100;
pub(super) const BASE_GLOBAL_COUNTER: u64 = 100;

macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

pub(super) fn epoll_create() -> io::Result<RawFd> {
    let fd = syscall!(epoll_create1(0))?;
    if let Ok(flags) = syscall!(fcntl(fd, libc::F_GETFD)) {
        let _ = syscall!(fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC))?;
    }

    Ok(fd)
}

pub(super) fn events_create() -> Vec<epoll_event> {
    Vec::with_capacity(STANDARD_CAPACITY as usize)
}

pub(super) fn epoll_wait(
    epoll_fd: RawFd,
    events: &mut Vec<epoll_event>,
) {
    debug_assert_eq!(STANDARD_CAPACITY as usize, events.capacity());
    events.clear();

    let events_count = match syscall!(epoll_wait(
            epoll_fd,
            events.as_mut_ptr(),
            STANDARD_CAPACITY,
            EPOLL_TIMEOUT_MS,
    )) {
        Ok(count) => count,
        Err(err) => {
            panic!("Syscall epoll_wait failed with {err}")
        }
    };

    unsafe { events.set_len(events_count as usize) };
}

pub fn close(fd: RawFd) {
    let _ = syscall!(close(fd));
}

pub(super) fn listener_read_event(key: u64) -> epoll_event {
    epoll_event {
        events: READ_FLAGS as u32,
        u64: key,
    }
}


pub(super) fn add_ctl(epoll_fd: RawFd, fd: RawFd, key: u64) -> io::Result<()> {
    let mut event = listener_read_event(key);

    syscall!(epoll_ctl(epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event))?;
    Ok(())
}

pub(super) fn rearm_fd(epoll_fd: RawFd, fd: RawFd, key: u64) {
    let mut event = listener_read_event(key);
    syscall!(epoll_ctl(epoll_fd, libc::EPOLL_CTL_MOD, fd, &mut event))
        .expect("Cannot rearm a non-existent fd");
}

// pub(super) fn remove_ctl(epoll_fd: RawFd, fd: RawFd) -> io::Result<()> {
//     syscall!(epoll_ctl(
//         epoll_fd,
//         libc::EPOLL_CTL_DEL,
//         fd,
//         std::ptr::null_mut()
//     ))?;
//     Ok(())
// }