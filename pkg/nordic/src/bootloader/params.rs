use common::fixed::vec::FixedVec;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use nordic_proto::proto::bootloader::*;
use peripherals::raw::nvmc::NVMC;
use protobuf::Message;

use crate::bootloader::flash::*;

pub fn read_bootloader_params() -> BootloaderParams {
    let params_block = unsafe { bootloader_params_data() };

    let checksum = u32::from_ne_bytes(*array_ref![params_block, 0, 4]);
    let length = u32::from_ne_bytes(*array_ref![params_block, 4, 4]);

    let mut data = &params_block[8..];
    if length as usize > data.len() {
        return BootloaderParams::default();
    }
    data = &data[0..(length as usize)];

    let expected_checksum = {
        let mut hasher = CRC32CHasher::new();
        hasher.update(data);
        hasher.finish_u32()
    };

    if checksum != expected_checksum {
        return BootloaderParams::default();
    }

    match BootloaderParams::parse(data) {
        Ok(v) => v,
        Err(_) => BootloaderParams::default(),
    }
}

pub fn write_bootloader_params(params: &BootloaderParams, nvmc: &mut NVMC) {
    let mut data = FixedVec::<u8, 256>::new();
    data.resize(8, 0); // Reserve space for the length and checksum.
    params.serialize_to(&mut data).unwrap();

    let checksum = {
        let mut hasher = CRC32CHasher::new();
        hasher.update(&data[8..]);
        hasher.finish_u32()
    };

    let length = (data.len() - 8) as u32;

    *array_mut_ref![data, 0, 4] = checksum.to_ne_bytes();
    *array_mut_ref![data, 4, 4] = length.to_ne_bytes();

    // NOTE: Flash writes must be word aligned so we pad up.
    data.resize(256, 0);

    write_to_flash(BOOTLOADER_PARAMS_OFFSET, &data, nvmc);
}
