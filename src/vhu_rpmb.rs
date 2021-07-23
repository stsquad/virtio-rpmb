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
//use vm_virtio::Queue;
//use vmm_sys_util::eventfd::EventFd;

use crate::rpmb::RpmbBackend;

type Result<T> = std::result::Result<T, Error>;
type VhostUserBackendResult<T> = std::result::Result<T, std::io::Error>;

#[derive(Debug)]
/// Errors related to vhost-user-rpmb daemon.
pub enum Error {
    /// Failed to handle event other than input event.
    HandleEventNotEpollIn,
    /// Failed to handle unknown event.
    HandleEventUnknownEvent,
    /// Guest gave us a write only descriptor that protocol says to read from.
    UnexpectedWriteOnlyDescriptor,
    /// Guest gave us a readable descriptor that protocol says to only write to.
    UnexpectedReadDescriptor,
    /// Invalid descriptor count
    UnexpectedDescriptorCount,
    /// Invalid descriptor
    UnexpectedDescriptorSize,
    /// Descriptor not found
    DescriptorNotFound,
    /// Descriptor read failed
    DescriptorReadFailed,
    /// Descriptor write failed
    DescriptorWriteFailed,
    /// Descriptor send failed
    DescriptorSendFailed,
}
impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vhost-device-i2c error: {:?}", self)
    }
}

impl convert::From<Error> for io::Error {
    fn from(e: Error) -> Self {
        io::Error::new(io::ErrorKind::Other, e)
    }
}

#[derive(Debug)]
pub struct VhostUserRpmb {
    backend: RpmbBackend,
    event_idx: bool,
    mem: Option<GuestMemoryAtomic<GuestMemoryMmap>>
}

// The device has been dropped.
// const KILL_EVENT: u16 = 2;
const QUEUE_SIZE: usize = 1024;
const NUM_QUEUES: usize = 1;

/*
 * Rpmb Message Parsing
 */
/*
#define VIRTIO_RPMB_REQ_PROGRAM_KEY        0x0001
#define VIRTIO_RPMB_REQ_GET_WRITE_COUNTER  0x0002
#define VIRTIO_RPMB_REQ_DATA_WRITE         0x0003
#define VIRTIO_RPMB_REQ_DATA_READ          0x0004
#define VIRTIO_RPMB_REQ_RESULT_READ        0x0005
*/
pub const VIRTIO_RPMB_REQ_PROGRAM_KEY: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RequestType {
    ProgramKey,
    Unsupported(u32),
}

// pub fn request_type(
//     mem: &GuestMemoryMmap,
//     desc_addr: GuestAddress,
// ) -> Result<RequestType, Error> {
//     let type_ = mem.read_obj(desc_addr).map_err(Error::GuestMemory)?;
//     match type_ {
//         VIRTIO_RPMB_REQ_PROGRAM_KEY => Ok(RequestType::ProgramKey),
//         t => Ok(RequestType::Unsupported(t)),
//     }
// }

// pub struct VirtIORPMBFrame {
//     pub stuff: [u8, 196],
    // pub key_mac[RPMB_KEY_MAC_SIZE]: u8,
    // uint8_t data[RPMB_BLOCK_SIZE];
    // uint8_t nonce[16];
    // /* remaining fields are big-endian */
    // uint32_t write_counter;
    // uint16_t address;
    // uint16_t block_count;
    // uint16_t result;
    // uint16_t req_resp;
// } __attribute__((packed));


/*
 * Core VhostUserRpmb methods
 */
impl VhostUserRpmb {
    pub fn new(backend: RpmbBackend) -> Result<Self> {
        Ok(VhostUserRpmb
           {
               backend: backend,
               event_idx: false,
               mem: None
           })
    }

    /*
     * Process the messages in the vring and dispatch replies
     */
    fn process_queue(&self, vring: &mut Vring) -> Result<bool> {
        // let mut reqs: Vec<VirtIORPMBFrame> = Vec::new();

        let requests: Vec<_> = vring
            .mut_queue()
            .iter()
            .map_err(|_| Error::DescriptorNotFound)?
            .collect();

        if requests.is_empty() {
            return Ok(true);
        }

        Ok(true)
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
        dbg!(&config);
        config
    }

    // fn set_config(&mut self, _offset: u32, _buf: &[u8]) -> result::Result<(), io::Error> {
    //     dbg!("set_config");
    //     Ok(())
    // }

    fn set_event_idx(&mut self, enabled: bool) {
        dbg!(self.event_idx = enabled);
    }

    fn update_memory(
        &mut self,
        mem: GuestMemoryAtomic<GuestMemoryMmap>,
    ) -> VhostUserBackendResult<()> {
        dbg!(self.mem = Some(mem));
        Ok(())
    }

    fn handle_event(
        &self,
        device_event: u16,
        evset: epoll::Events,
        vrings: &[Arc<RwLock<Vring>>],
        _thread_id: usize,
    ) -> VhostUserBackendResult<bool> {
        dbg!(device_event);
        dbg!(evset);

        if evset != epoll::Events::EPOLLIN {
            return Err(Error::HandleEventNotEpollIn.into());
        }

        match device_event {
            0 => {
                let mut vring = vrings[0].write().unwrap();

                if self.event_idx {
                    // vm-virtio's Queue implementation only checks avail_index
                    // once, so to properly support EVENT_IDX we need to keep
                    // calling process_queue() until it stops finding new
                    // requests on the queue.
                    loop {
                        vring.mut_queue().disable_notification().unwrap();

                        self.process_queue(&mut vring)?;
                        if !vring.mut_queue().enable_notification().unwrap() {
                            break;
                        }
                    }
                } else {
                    // Without EVENT_IDX, a single call is enough.
                    self.process_queue(&mut vring)?;
                }
            }
            _ => {
                dbg!("unhandled device_event:", device_event);
                return Err(Error::HandleEventUnknownEvent.into());
            }
        }
        Ok(false)
    }
}
