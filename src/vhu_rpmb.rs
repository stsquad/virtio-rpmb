/*
 * vhost user rpmb device
 *
 * This encapsulates all vhost user message handling.
 */
use std::sync::{Arc, RwLock};
use std::{convert, error, fmt, io};

use vhost::vhost_user::message::*;
use vhost_user_backend::{VhostUserBackend, Vring};
use virtio_bindings::bindings::virtio_net::{
    VIRTIO_F_VERSION_1, VIRTIO_F_NOTIFY_ON_EMPTY
};
use virtio_bindings::bindings::virtio_ring::{
    VIRTIO_RING_F_EVENT_IDX, VIRTIO_RING_F_INDIRECT_DESC,
};
use vm_memory::{GuestMemoryAtomic, GuestMemoryMmap};
//use vmm_sys_util::eventfd::EventFd;

use crate::rpmb::RpmbBackend;

// type VhostUserRpmbResult<T> = std::result::Result<T, std::io::Error>;
type VhostUserBackendResult<T> = std::result::Result<T, std::io::Error>;

#[derive(Debug)]
pub struct VhostUserRpmb {
    backend: RpmbBackend,
    event_idx: bool,
    mem: Option<GuestMemoryAtomic<GuestMemoryMmap>>
}

#[derive(Debug)]
enum Error {
    /// Failed to handle event other than input event.
    HandleEventNotEpollIn,
    /// Failed to handle unknown event.
    HandleEventUnknownEvent,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vhost-user-rpmb error: {:?}", self)
    }
}

impl error::Error for Error {}

impl convert::From<Error> for io::Error {
    fn from(e: Error) -> Self {
        io::Error::new(io::ErrorKind::Other, e)
    }
}

// The device has been dropped.
// const KILL_EVENT: u16 = 2;
const QUEUE_SIZE: usize = 1024;
const NUM_QUEUES: usize = 1;

/*
 * Core VhostUserRpmb methods
 */
impl VhostUserRpmb {
    pub fn new(backend: RpmbBackend) -> Result<Self, std::io::Error> {
        Ok(VhostUserRpmb
           {
               backend: backend,
               event_idx: false,
               mem: None
           })
    }
}

/*
 * VhostUserBackend trait methods
 */
impl VhostUserBackend for VhostUserRpmb {
    fn num_queues(&self) -> usize {
        NUM_QUEUES
    }

    fn max_queue_size(&self) -> usize {
        QUEUE_SIZE
    }

    fn features(&self) -> u64 {
        /* this set matches the current libvhost defaults except VHOST_F_LOG_ALL*/
        let feat: u64 = 1 << VIRTIO_F_VERSION_1
            | 1 << VIRTIO_F_NOTIFY_ON_EMPTY
            | 1 << VIRTIO_RING_F_INDIRECT_DESC
            | 1 << VIRTIO_RING_F_EVENT_IDX
            | VhostUserVirtioFeatures::PROTOCOL_FEATURES.bits();
        dbg!(format!("{:#018x}", &feat));
        dbg!(format!("{:#018x}", VhostUserVirtioFeatures::PROTOCOL_FEATURES.bits()));
        feat
    }

    fn protocol_features(&self) -> VhostUserProtocolFeatures {
        let pfeat: VhostUserProtocolFeatures = VhostUserProtocolFeatures::REPLY_ACK
            | VhostUserProtocolFeatures::CONFIG
            | VhostUserProtocolFeatures::RESET_DEVICE
            | VhostUserProtocolFeatures::STATUS
            | VhostUserProtocolFeatures::MQ;
        dbg!(pfeat);
        pfeat
    }

    fn get_config(&self, _offset: u32, _size: u32) -> Vec<u8> {
        let config: Vec<u8> = vec![self.backend.get_capacity(), 1, 1];
        dbg!("get_config: {}", &config);
        config
    }

    // fn set_config(&mut self, _offset: u32, _buf: &[u8]) -> result::Result<(), io::Error> {
    //     dbg!("set_config");
    //     Ok(())
    // }

    fn set_event_idx(&mut self, enabled: bool) {
        dbg!("set_event_idx: {}", enabled);
        self.event_idx = enabled;
    }

    fn update_memory(
        &mut self,
        mem: GuestMemoryAtomic<GuestMemoryMmap>,
    ) -> VhostUserBackendResult<()> {
        self.mem = Some(mem);
        dbg!("in update_memory");
        Ok(())
    }

    fn handle_event(
        &self,
        _device_event: u16,
        evset: epoll::Events,
        _vrings: &[Arc<RwLock<Vring>>],
        _thread_id: usize,
    ) -> VhostUserBackendResult<bool> {
        dbg!(_device_event);
        dbg!(evset);

        if evset != epoll::Events::EPOLLIN {
            return Err(Error::HandleEventNotEpollIn.into());
        }


        Ok(false)
    }
}
