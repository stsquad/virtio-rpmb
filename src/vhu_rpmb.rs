/*
 * vhost user rpmb device
 *
 * This encapsulates all vhost user message handling.
 */
use crate::rpmb::*;
use std::mem::size_of;
use std::sync::{Arc, RwLock};
use std::{convert, error, fmt, io};
use core::fmt::Debug;
use arrayvec::ArrayVec;
use log::{info, trace, warn, error};

use vhost::vhost_user::message::*;
use vhost_user_backend::{VhostUserBackend, Vring};
use virtio_bindings::bindings::virtio_net::{
    VIRTIO_F_VERSION_1, VIRTIO_F_NOTIFY_ON_EMPTY
};
use virtio_bindings::bindings::virtio_ring::{
    VIRTIO_RING_F_EVENT_IDX, VIRTIO_RING_F_INDIRECT_DESC,
};
use vm_memory::{Be16, Be32, Bytes, ByteValued, GuestMemoryAtomic, GuestMemoryMmap};
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
pub const VIRTIO_RPMB_REQ_PROGRAM_KEY:  u16 = 0x0001;
pub const VIRTIO_RPMB_REQ_RESULT_READ:  u16 = 0x0005;
pub const VIRTIO_RPMB_RESP_PROGRAM_KEY: u16 = 0x0100;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RequestType {
    ProgramKey,
    Unsupported(u32),
}

// #define VIRTIO_RPMB_RES_OK                     0x0000
// w
// #define VIRTIO_RPMB_RES_AUTH_FAILURE           0x0002
// #define VIRTIO_RPMB_RES_COUNT_FAILURE          0x0003
// #define VIRTIO_RPMB_RES_ADDR_FAILURE           0x0004
// #define VIRTIO_RPMB_RES_WRITE_FAILURE          0x0005
// #define VIRTIO_RPMB_RES_READ_FAILURE           0x0006
// #define VIRTIO_RPMB_RES_NO_AUTH_KEY            0x0007
// #define VIRTIO_RPMB_RES_WRITE_COUNTER_EXPIRED  0x0080
pub const VIRTIO_RPMB_RES_OK: u16 = 0x0000;
pub const VIRTIO_RPMB_RES_GENERAL_FAILURE: u16 = 0x0001;
pub const VIRTIO_RPMB_RES_WRITE_FAILURE: u16 = 0x0005;

pub enum RequestResultType {
    Ok,
    GeneralFailure
}

#[derive(Debug)]
struct ResultReqResp(u16, u16);

#[derive(Debug)]
enum RequestResponse {
    NoResponse,
    PendingResponse { req_resp: u16, result: u16 },
    Response(VirtIORPMBFrame)
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

#[derive(Copy, Clone)]
#[repr(C, packed)]
struct VirtIORPMBFrame {
    stuff: [u8; 196],
    key_mac: [u8; RPMB_KEY_MAC_SIZE],
    data: [u8; RPMB_BLOCK_SIZE],
    nonce: [u8; 16],
    write_counter: Be32,
    address: Be16,
    block_count: Be16,
    result: Be16,
    req_resp: Be16
}

/*
 * "Default is not implemented for arrays of length > 32
 * for annoying backwards compatibility reasons" - so we must do it
 * ourselves for the frame array.
 */

impl Default for VirtIORPMBFrame {
    fn default() -> Self {
        VirtIORPMBFrame {
            stuff: [0; 196],
            key_mac: [0; RPMB_KEY_MAC_SIZE],
            data: [0; RPMB_BLOCK_SIZE],
            nonce: [0; 16],
            write_counter: From::from(0),
            address: From::from(0),
            block_count: From::from(0),
            result: From::from(0),
            req_resp: From::from(0)
        }
    }
}

impl Debug for VirtIORPMBFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let res_copy = { self.result };
        let req_resp_copy = { self.req_resp };
        let data_sample = &self.data[0 .. 16];
        f.debug_struct("VirtIORPMBFrame")
            .field("key_mac", &self.key_mac)
            .field("data", &data_sample)
            .field("nonce", &self.nonce)
            .field("result", &res_copy)
            .field("req_resp", &req_resp_copy)
         .finish_non_exhaustive()
    }
}

unsafe impl ByteValued for VirtIORPMBFrame {}

/* Implement some frame builders for sending our results back */
impl VirtIORPMBFrame {
    fn result(response:u16, result: u16) -> Self {
        VirtIORPMBFrame {
            stuff: [0; 196],
            key_mac: [0; RPMB_KEY_MAC_SIZE],
            data: [0; RPMB_BLOCK_SIZE],
            nonce: [0; 16],
            write_counter: From::from(0),
            address: From::from(0),
            block_count: From::from(0),
            result: From::from(result),
            req_resp: From::from(response)
        }
    }
}

/*
 * Core VhostUserRpmb methods
 */
impl VhostUserRpmb {
    pub fn new(backend: RpmbBackend) -> Result<Self> {
        Ok(VhostUserRpmb
           {
               backend,
               event_idx: false,
               mem: None
           })
    }

    fn program_key(&self, frame: VirtIORPMBFrame) -> RequestResponse {
        let result = if frame.block_count.to_native() != 1 {
           VIRTIO_RPMB_RES_GENERAL_FAILURE
        } else {
            match self.backend.program_key(ArrayVec::from(frame.key_mac)) {
                Ok(_) => {
                    VIRTIO_RPMB_RES_OK
                }
                Err(_) => {
                    VIRTIO_RPMB_RES_WRITE_FAILURE
                }
            }
        };
        RequestResponse::PendingResponse{req_resp: VIRTIO_RPMB_RESP_PROGRAM_KEY, result}
    }
    
    /*
     * Process the messages in the vring and dispatch replies
     */
    fn process_queue(&self, vring: &mut Vring) -> Result<bool> {
        // let mut reqs: Vec<VirtIORPMBFrame> = Vec::new();
        let mut pending = RequestResponse::NoResponse;

        let requests: Vec<_> = vring
            .mut_queue()
            .iter()
            .map_err(|_| Error::DescriptorNotFound)?
            .collect();

        if requests.is_empty() {
            return Ok(true);
        }

        /*
         * Iterate over the requests and handle the messages.
         * Generally we expect at least two descriptors, the request
         * itself and the descriptors for the response. The other form
         * is a request followed by a request for a result and then
         * the buffer for the reply.
         */
        for desc_chain in requests.clone() {
            let buffers: Vec<_> = desc_chain.clone().collect();
            let mut consumed = 0;

            trace!("Buffers: {:x?}", &buffers);

            if buffers.len() < 2 {
                return Err(Error::UnexpectedDescriptorCount);
            }

            let (writeable, readable): (Vec<_>, Vec<_>) = buffers.into_iter().partition(|b| b.is_write_only());

            /* Process the incoming frames */
            for b in &readable {

                /* All frames should be the same size */
                if b.len() as usize != size_of::<VirtIORPMBFrame>() {
                    error!("Unexpected frame size: {}", b.len());
                    return Err(Error::UnexpectedDescriptorSize);
                }

                /* Convert the descriptor into something we can work with */
                let frame = desc_chain
                    .memory()
                    .read_obj::<VirtIORPMBFrame>(b.addr())
                    .map_err(|_| Error::DescriptorReadFailed)?;


                let req_resp = frame.req_resp.to_native();
                trace!("Incoming frame: {:x?} => req_resp {:x?}", frame, req_resp);

                /* Dispatch request frames to their handlers */
                let res: RequestResponse = match req_resp {
                    VIRTIO_RPMB_REQ_PROGRAM_KEY => {
                        self.program_key(frame)
                    }
                    VIRTIO_RPMB_REQ_RESULT_READ => {
                        match pending {
                            RequestResponse::PendingResponse{req_resp, result} => {
                                pending = RequestResponse::NoResponse;
                                RequestResponse::Response(VirtIORPMBFrame::result(req_resp, result))
                            }
                            _ => {
                                RequestResponse::NoResponse
                            }
                        }
                    }
                    _ => {
                        warn!("Un-handled req_resp {:x?}", req_resp);
                        RequestResponse::NoResponse
                    }
                };

                trace!("Result: {:x?}", &res);

                /*
                 * After we have handled the frame we either have a
                 * response to send, a deferred status that might be
                 * queried later or nothing to send at all.
                 */

                let replied_bytes = match res {
                    RequestResponse::Response(frame) => {

                        // we really should take one
                        let result_buf = writeable[0];

                        desc_chain
                            .memory()
                            .write_obj::<VirtIORPMBFrame>(frame, result_buf.addr())
                            .map_err(|_| Error::DescriptorWriteFailed)?;

                        size_of::<VirtIORPMBFrame>() as u32
                    }
                    // No immediate response, wait for query
                    RequestResponse::PendingResponse{req_resp, result} => {
                        pending = RequestResponse::PendingResponse{req_resp,
                                                             result};
                        0
                    }
                    _ => {
                        info!("no response needed");
                        0
                    }
                };

                consumed += replied_bytes;

            } // for each readable frame

            if vring
                .mut_queue()
                .add_used(desc_chain.head_index(), consumed)
                .is_err()
            {
                warn!("Couldn't return used consumed descriptors to the ring");
            }


            // Send notification once all the requests are processed
            vring
                .signal_used_queue()
                .map_err(|_| Error::DescriptorSendFailed)?;
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
        info!("{:#018x}", &feat);
        info!("{:#018x}", VhostUserVirtioFeatures::PROTOCOL_FEATURES.bits());
        feat
    }

    fn protocol_features(&self) -> VhostUserProtocolFeatures {
        let pfeat: VhostUserProtocolFeatures = VhostUserProtocolFeatures::REPLY_ACK
            | VhostUserProtocolFeatures::CONFIG
            | VhostUserProtocolFeatures::RESET_DEVICE
            | VhostUserProtocolFeatures::STATUS
            | VhostUserProtocolFeatures::MQ;
        info!("protocol features: {:?}", pfeat);
        pfeat
    }

    fn get_config(&self, _offset: u32, _size: u32) -> Vec<u8> {
        let config: Vec<u8> = vec![self.backend.get_capacity(), 1, 1];
        info!("{:?}", &config);
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
        Ok(())
    }

    fn handle_event(
        &self,
        device_event: u16,
        evset: epoll::Events,
        vrings: &[Arc<RwLock<Vring>>],
        _thread_id: usize,
    ) -> VhostUserBackendResult<bool> {
        trace!("{}", device_event);
        trace!("{:?}", evset);

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
                warn!("unhandled device_event: {}", device_event);
                return Err(Error::HandleEventUnknownEvent.into());
            }
        }
        Ok(false)
    }
}
