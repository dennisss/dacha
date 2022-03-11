use common::const_default::ConstDefault;
use common::errors::*;

use nordic_proto::proto::net::NetworkConfig;
use peripherals::storage::BlockHandle;
use peripherals::storage::BlockStorage;
use peripherals::storage::BlockStorageError;
use protobuf::Message;

use crate::eeprom::Microchip24XX256;

/// Random id to uniquely identify the config block.
pub const NETWORK_CONFIG_BLOCK_ID: u32 = 0x2EDFB789;

/// Conservative maximum size of the serialized NetworkConfig proto now and in
/// the future if we add more fields.
///
/// NOTE: Changing this may break any existing EEPROMs we have written to.
pub const NETWORK_CONFIG_MAX_SIZE: usize = 1024;

pub struct NetworkConfigStorage {
    block_handle: BlockHandle<'static, Microchip24XX256>,
}

impl NetworkConfigStorage {
    pub async fn open(block_storage: &'static BlockStorage<Microchip24XX256>) -> Result<Self> {
        let block_handle = block_storage
            .open(NETWORK_CONFIG_BLOCK_ID, NETWORK_CONFIG_MAX_SIZE)
            .await?;
        Ok(Self { block_handle })
    }

    /// Returns whether or not a value was present in storage and has been
    /// loaded into the given config,
    pub async fn read(&mut self, config: &mut NetworkConfig) -> Result<bool> {
        let mut data = [0u8; 256];
        let n = match self.block_handle.read(&mut data).await {
            Ok(n) => n,
            Err(e) => {
                if let Some(BlockStorageError::NoValidData) = e.downcast() {
                    return Ok(false);
                }

                return Err(e);
            }
        };

        // Clear any data in the old config.
        *config = NetworkConfig::DEFAULT;
        config.parse_merge(&data[0..n])?;

        Ok(true)
    }

    pub async fn write(&mut self, config: &NetworkConfig) -> Result<()> {
        let mut data = common::collections::FixedVec::new([0u8; 256]);
        config.serialize_to(&mut data)?;
        self.block_handle.write(&data).await?;
        Ok(())
    }
}
