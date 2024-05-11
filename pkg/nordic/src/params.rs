use common::errors::*;
use executor::lock;
use executor::sync::AsyncMutex;
use peripherals::blob::{BlobHandle, BlobMemoryController, BlobRegistry, BlobStorage};
use peripherals::raw::nvmc::NVMC;
use protobuf::{Message, StaticMessage};

use crate::bootloader::flash::{
    application_params_data, flash_page_size, write_to_flash, FLASH_ERASED_WORD_VALUE,
};

pub const NETWORK_CONFIG_ID: u32 = 0x861C8E73;
pub const NETWORK_STATE_ID: u32 = 0xB0A4A986;

/// Stores small application parameters robustly in NRF application flash space.
pub struct ParamsStorage {
    blobs: AsyncMutex<BlobStorage<AppParamsMemoryController, [BlobHandle; 2]>>,
}

impl ParamsStorage {
    /// TODO: This is only safe if at most once ParamsStorage instance is ever
    /// created.
    pub fn create(nvmc: NVMC) -> Result<Self> {
        let memory = AppParamsMemoryController { nvmc };

        let registry = [
            BlobHandle::new(NETWORK_CONFIG_ID),
            BlobHandle::new(NETWORK_STATE_ID),
        ];

        let blobs = BlobStorage::create(memory, registry)?;

        Ok(Self {
            blobs: AsyncMutex::new(blobs),
        })
    }

    /// Returns true if and only if a valid value was found in storage.
    pub async fn read_into_proto<M: StaticMessage>(
        &self,
        param_id: u32,
        proto: &mut M,
    ) -> Result<bool> {
        let blobs = self.blobs.lock().await?.read_exclusive();

        let data = match blobs.get(param_id)? {
            Some(v) => v,
            None => return Ok(false),
        };

        // Clear any data in the old config.
        *proto = M::default();

        Ok(proto.parse_merge(data).is_ok())
    }

    pub async fn write_proto<M: Message>(&self, param_id: u32, proto: &M) -> Result<()> {
        let mut data = common::fixed::vec::FixedVec::<u8, 256>::new();
        proto.serialize_to(&mut data)?;

        lock!(blobs <= self.blobs.lock().await?, {
            blobs.write(param_id, &data)
        })
    }
}

struct AppParamsMemoryController {
    nvmc: NVMC,
}

impl BlobMemoryController for AppParamsMemoryController {
    fn len(&self) -> usize {
        unsafe { application_params_data() }.len() as usize
    }

    fn page_size(&self) -> usize {
        flash_page_size() as usize
    }

    fn write_alignment(&self) -> usize {
        core::mem::size_of::<u32>()
    }

    fn can_write_to_offset(&self, mut offset: usize) -> bool {
        let page_size = flash_page_size() as usize;
        let page_offset = offset % page_size;
        if page_offset == 0 {
            return true;
        }

        if offset % 4 != 0 {
            return false;
        }

        let next_page = offset + (page_size - page_offset);
        let data = self.get();
        for word in data[offset..next_page].chunks(4) {
            let word = u32::from_le_bytes(*array_ref![word, 0, 4]);
            if word != FLASH_ERASED_WORD_VALUE {
                return false;
            }
        }

        true
    }

    fn get<'a>(&'a self) -> &'a [u8] {
        unsafe { application_params_data() }
    }

    fn read(&self, offset: usize, out: &mut [u8]) {
        let data = self.get();
        out.copy_from_slice(&data[offset..(offset + out.len())]);
    }

    fn write(&mut self, offset: usize, data: &[u8]) {
        let addr = unsafe { core::mem::transmute::<_, u32>(self.get().as_ptr()) } + (offset as u32);
        write_to_flash(addr, data, &mut self.nvmc);
    }
}
