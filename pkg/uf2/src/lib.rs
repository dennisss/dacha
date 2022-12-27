#![no_std]

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;
#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[macro_use]
extern crate common;

use core::mem::{size_of, transmute};

pub const UF2_BLOCK_SIZE: usize = 512;

pub const UF2_MAGIC_START_0: u32 = 0x0A324655; // "UF2\n"
pub const UF2_MAGIC_START_1: u32 = 0x9E5D5157;
pub const UF2_MAGIC_END: u32 = 0x0AB16F30;

/// All fields are 32-bit little endian.
#[repr(C)]
pub struct UF2Block {
    pub magic_start_0: u32,
    pub magic_start_1: u32,
    pub flags: UF2Flags,
    pub target_addr: u32,

    /// NOTE: Typically this will have 256 bytes
    pub payload_size: u32,

    pub block_number: u32,
    pub num_blocks: u32,
    pub file_size: u32, // or family_id

    /// Payload with everything beyond the payload_size padded with 0s.    
    pub data: [u8; 476],

    pub magic_end: u32,
}

define_bit_flags!(
    UF2Flags u32 {
        NotMainFlash = 0x00000001,
        FileContainer = 0x00001000,
        FamilyId = 0x00002000,
        MD5ChecksumPresent = 0x00004000,
        ExtensionTagsPresent = 0x00008000
    }
);

impl Default for UF2Block {
    fn default() -> Self {
        Self {
            magic_start_0: UF2_MAGIC_START_0,
            magic_start_1: UF2_MAGIC_START_1,
            flags: UF2Flags::empty(),
            target_addr: 0,
            payload_size: 0,
            block_number: 0,
            num_blocks: 0,
            file_size: 0,
            data: [0u8; 476],
            magic_end: UF2_MAGIC_END,
        }
    }
}

impl UF2Block {
    pub fn payload(&self) -> &[u8] {
        &self.data[0..(self.payload_size as usize)]
    }

    #[cfg(target_endian = "little")]
    pub fn cast_from<'a>(data: &'a [u8]) -> Option<&'a Self> {
        if data.len() != size_of::<Self>() {
            return None;
        }

        let block = unsafe { transmute::<_, &Self>(data.as_ptr()) };

        if block.magic_start_0 != UF2_MAGIC_START_0
            || block.magic_start_1 != UF2_MAGIC_START_1
            || block.magic_end != UF2_MAGIC_END
            || block.payload_size >= block.data.len() as u32
        {
            return None;
        }

        // TODO: validate that the data is zero padded.

        Some(block)
    }

    #[cfg(target_endian = "little")]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(transmute(self), size_of::<Self>()) }
    }
}

// TODO: Keep in sync with https://github.com/microsoft/uf2/blob/master/utils/uf2families.json
#[cfg(target_label = "nrf52840")]
pub const MY_FAMILY_ID: u32 = 0xada52840;

#[cfg(target_label = "nrf52833")]
pub const MY_FAMILY_ID: u32 = 0x621e937a;

/*
let mut uf2 = vec![];

{
    let total_num_blocks = common::ceil_div(flash_end - flash_start, 256);

    let mut flash_i = 0;
    while flash_i < flash_contents.len() {
        uf2.extend_from_slice(&(0x0A324655 as u32).to_le_bytes());
        uf2.extend_from_slice(&(0x9E5D5157 as u32).to_le_bytes());
        uf2.extend_from_slice(&(0 as u32).to_le_bytes()); // flags
        uf2.extend_from_slice(&((flash_start + flash_i) as u32).to_le_bytes());
        uf2.extend_from_slice(&(256 as u32).to_le_bytes());
        uf2.extend_from_slice(&((flash_i / 256) as u32).to_le_bytes());
        uf2.extend_from_slice(&(total_num_blocks as u32).to_le_bytes());
        uf2.extend_from_slice(&(0 as u32).to_le_bytes());

        let mut data = [0u8; 476];
        let n = std::cmp::min(256, flash_contents.len() - flash_i);
        data[0..n].copy_from_slice(&flash_contents[flash_i..(flash_i + n)]);
        uf2.extend_from_slice(&data);

        uf2.extend_from_slice(&(0x0AB16F30 as u32).to_le_bytes());

        flash_i += 256;
    }
}

file::write(project_path!("rp2040.uf2"), &uf2).await?;
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uf2_block_size() {
        assert_eq!(core::mem::size_of::<UF2Block>(), UF2_BLOCK_SIZE);
    }
}
