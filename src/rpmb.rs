/*
 * rpmb backend device
 *
 * This encapsulates all the state for the RPMB device
 *
 */

use std::path::Path;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{Result, Error, ErrorKind};
use std::convert::TryFrom;
use memmap::{MmapMut, MmapOptions};

const KB: u64 = 1024;
const UNIT_128KB: u64 = KB * 128;
const MAX_RPMB_SIZE: u64 = UNIT_128KB * 128;

pub const RPMB_KEY_MAC_SIZE: usize = 32;
pub const RPMB_BLOCK_SIZE: usize = 256;


#[derive(Debug)]
pub struct RpmbBackend {
    image: File,
    mmap: MmapMut,
    capacity: u8,
    write_count: u32,
    read_count: u32,
}

impl RpmbBackend {
    pub fn new(image_path: &Path) -> Result<RpmbBackend> {

        let image = OpenOptions::new().read(true).write(true).open(image_path)?;
        let metadata = image.metadata()?;

        let mut len = metadata.len();
        if len > MAX_RPMB_SIZE {
            println!("{} is larger than maximum size supported", image_path.display());
            len = MAX_RPMB_SIZE;
        }
        let mmap = unsafe { MmapOptions::new()
                            .len(len as usize)
                            .map_mut(&image)? };

        let capacity:u8 = u8::try_from(len / UNIT_128KB)
            .map_err(|_e| Error::new(ErrorKind::InvalidData, "More capacity than can be accessed!"))?;

        Ok(RpmbBackend {
            image,
            mmap,
            capacity: capacity,
            write_count: 0,
            read_count: 0
        })
    }

    pub fn get_capacity(&self) -> u8 {
        self.capacity
    }
}
