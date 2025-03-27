use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::connection::event_handle::standalone_handle::standalone_handle_epoll_channels::StandaloneHandleEpollChannels;
use crate::connection::event_handle::standalone_handle::standalone_handle_pool_channels::StandaloneHandlePoolChannels;
use crate::messages::{Message, TimedMessage};
use crossbeam_channel::{unbounded, Receiver, Sender};

pub(in super::super) struct StandaloneHandleBuilder {
    pool_channels: Option<StandaloneHandlePoolChannels>,
    epoll_channels: Option<StandaloneHandleEpollChannels>,
    handle: StandaloneHandle
}

impl StandaloneHandleBuilder {
    pub(in super::super) fn new(connection_id: ConnectionIdentifier) -> Self {
        let (to_handle_from_pool, from_pool_to_handle) = unbounded();
        let (to_pool_from_handle, from_handle_to_pool) = unbounded();
        let (to_pool_from_epoll, from_epoll_to_pool) = unbounded();
        let (handle_state_sender, handle_state_receiver) = unbounded();

        StandaloneHandleBuilder {
            pool_channels: Some(
                StandaloneHandlePoolChannels::new(
                    connection_id,
                    from_epoll_to_pool,
                    handle_state_receiver,
                    from_handle_to_pool,
                    to_handle_from_pool,
                )
            ),
            epoll_channels: Some(
                StandaloneHandleEpollChannels::new(
                    connection_id,
                    handle_state_sender,
                    to_pool_from_epoll
                )
            ),
            handle: StandaloneHandle {
                from_pool_to_handle,
                to_pool_from_handle,
            },
        }
    }

    pub(in super::super) fn take_handle_pool_channels(&mut self) -> StandaloneHandlePoolChannels {
        self.pool_channels.take()
            .expect("This can only be done once.")
    }

    pub(in super::super) fn take_handle_epoll_channels(&mut self) -> StandaloneHandleEpollChannels {
        self.epoll_channels.take()
            .expect("This can only be done once.")
    }

    pub(in super::super) fn into_handle(self) -> StandaloneHandle {
        self.handle
    }

}

pub struct StandaloneHandle {
    from_pool_to_handle: Receiver<Message>,
    to_pool_from_handle: Sender<TimedMessage>
}

impl StandaloneHandle {
    pub fn send_message_to_controller(&self, message: Message) {
        self.to_pool_from_handle.send(
            TimedMessage::of(message)
        ).unwrap()
    }

    pub fn recv_controller_message(&self) -> &Receiver<Message> {
        &self.from_pool_to_handle
    }
}
