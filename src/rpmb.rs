/*
 * rpmb backend device
 *
 * This encapsulates all the state for the RPMB device
 *
 */

use std::path::Path;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{Result};
use memmap::{MmapMut, MmapOptions};

const MAX_RPMB_SIZE: u64 = 1024 * 128 * 256;

pub struct RpmbBackend {
    image: File,
    mmap: MmapMut,
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

        Ok(RpmbBackend {
            image,
            mmap,
            write_count: 0,
            read_count: 0
        })
    }

}
