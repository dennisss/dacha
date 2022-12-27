use alloc::string::String;
use std::time::Duration;

use common::errors::*;

use crate::descriptor_iter::*;
use crate::descriptors::InterfaceClass;
use crate::descriptors::SetupPacket;
use crate::dfu::descriptors::*;
use crate::linux::Context;
use crate::linux::Device;
use crate::selector::DeviceSelector;
use crate::DeviceEntry;

// TODO: Enforce max timeouts on all USB operations.

// Range of block sizes we will accept when uploading or downloading firmware.
const MIN_BLOCK_SIZE: u16 = 64;
const MAX_BLOCK_SIZE: u16 = 4096;

/// Interface for interacting with a remote device connected to the current host
/// computer.
pub struct DFUHost {
    context: Context,
    device_selector: DeviceSelector,
}

impl DFUHost {
    /// Creates a new instance based on a selector for a device.
    /// The given selector must only select a single device.
    pub fn create(device_selector: DeviceSelector) -> Result<Self> {
        let context = Context::create()?;
        Ok(Self {
            context,
            device_selector,
        })
    }

    /// Writes the given firmware data to an attached DFU device.
    pub async fn download(&mut self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Err(err_msg("Firmware empty"));
        }

        let mut device = self.find_dfu_mode_device().await?.open().await?;

        println!("Found DFU Mode device");

        let block_size = device
            .metadata
            .functional_descriptor
            .wTransferSize
            .min(MAX_BLOCK_SIZE);
        if block_size < MIN_BLOCK_SIZE {
            return Err(err_msg("Block size too small"));
        }

        println!("Block Size: {}", block_size);

        // TODO: Start with an initial GET_STATUS request to get the bwPollTimeout
        // value.

        // TODO: Set a global timeout of 10 seconds for all individual USB operations.

        // Stop any previous operations being performed by the device.
        device.abort().await?;

        for (block_num, block_data) in data.chunks(block_size as usize).enumerate() {
            println!("Download block #{}", block_num);

            device
                .download_block(block_num as u16 /* will wrap */, block_data)
                .await?;
        }

        println!("Trigger manifestation");

        // Final download. Device will enter manifestation state.
        device.download_block(0, &[]).await?;

        println!("Downloads done!");

        if device
            .metadata
            .functional_descriptor
            .bmAttributes
            .contains(DFUAttributes::bitManifestationTolerant)
        {
            // In dfuMANIFEST-SYNC state.

            // TODO: Issue a DFU_GETSTATUS. Once it is done, we become in the
            // idle state.
        } else if !device
            .metadata
            .functional_descriptor
            .bmAttributes
            .contains(DFUAttributes::bitWillDetach)
        {
            device.device.reset()?;
        }

        Ok(())
    }

    /// Finds a USB device in DFU mode. If there is a runtime device, it is
    /// detached and forced into DFU mode.
    async fn find_dfu_mode_device(&mut self) -> Result<DFUHostDeviceEntry> {
        for iter in 0..4 {
            let device_entry = match self.find_device().await? {
                Some(v) => v,
                None => {
                    if iter == 0 {
                        return Err(err_msg("No DFU device found"));
                    }

                    // Still waiting for restart.
                    executor::sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };

            if device_entry.metadata.protocol == DFUInterfaceProtocol::DFUMode {
                return Ok(device_entry);
            }

            // TODO: Remember the device's bus and port path so that we can reference the
            // same physical port even if the device id changes when entering the
            // bootloader.
            println!("Found DFU runtime device. Detaching...");

            let mut device = device_entry.open().await?;

            println!("Opened!");

            device.device.write_control(
                SetupPacket {
                    bmRequestType: 0b00100001,
                    bRequest: DFURequestType::DFU_DETACH as u8,
                    wValue: 100.min(device.metadata.functional_descriptor.wDetachTimeOut), // wTimeout
                    wIndex: device.metadata.interface_num as u16,
                    wLength: 0,
                },
                &[],
            ).await?;

            if !device
                .metadata
                .functional_descriptor
                .bmAttributes
                .contains(DFUAttributes::bitWillDetach)
            {
                device.device.reset()?;
            }
        }

        Err(err_msg(
            "Exceeded max number of iterations in detaching device",
        ))
    }

    /// Finds a USB device which both matches the user's selection criteria and
    /// supports DFU.
    async fn find_device(&self) -> Result<Option<DFUHostDeviceEntry>> {
        let mut devices = self.context.enumerate_devices().await?;

        let mut found_device = None;

        for device_entry in devices {
            if !self.device_selector.matches(&device_entry)? {
                continue;
            }

            if let Some(entry) = self.extract_device_dfu_entry(device_entry)? {
                if found_device.is_some() {
                    return Err(err_msg("Multiple matching DFU devices found"));
                }

                found_device = Some(entry);
            }
        }

        Ok(found_device)
    }

    fn extract_device_dfu_entry(
        &self,
        device_entry: DeviceEntry,
    ) -> Result<Option<DFUHostDeviceEntry>> {
        let mut current_config_value = None;
        let mut current_interface_number = None;
        let mut current_dfu_protocol = None;

        let mut descs = device_entry.descriptors();
        while let Some(desc) = descs.next() {
            let desc = match desc {
                Ok(v) => v,
                Err(_) => {
                    // TODO: Print which device this is.
                    return Ok(None);
                }
            };

            match desc {
                Descriptor::Configuration(cfg) => {
                    current_config_value = Some(cfg.bConfigurationValue);
                }
                Descriptor::Interface(iface) => {
                    current_interface_number = Some(iface.bInterfaceNumber);

                    if iface.bInterfaceClass == InterfaceClass::ApplicationSpecific.to_value()
                        && iface.bInterfaceSubClass == DFU_INTERFACE_SUBCLASS
                    {
                        current_dfu_protocol =
                            Some(DFUInterfaceProtocol::from_value(iface.bInterfaceProtocol));
                    } else {
                        current_dfu_protocol = None;
                    }
                }
                Descriptor::Unknown(desc) => {
                    if current_dfu_protocol.is_some()
                        && desc.raw_type() == DFU_FUNCTIONAL_DESCRIPTOR_TYPE
                    {
                        let functional_descriptor = desc.decode::<DFUFunctionalDescriptor>()?;
                        let config_value =
                            current_config_value.ok_or_else(|| err_msg("Missing config value"))?;
                        let interface_num = current_interface_number
                            .ok_or_else(|| err_msg("Missing interface num"))?;

                        let protocol =
                            current_dfu_protocol.ok_or_else(|| err_msg("Missing dfu protocol"))?;

                        // TODO: Verify a single device doesn't have multiple DFU
                        // interfaces/configs.

                        // drop(desc);
                        drop(descs);

                        return Ok(Some(DFUHostDeviceEntry {
                            device_entry,
                            metadata: DFUHostDeviceMetadata {
                                protocol,
                                config_value,
                                interface_num,
                                functional_descriptor,
                            },
                        }));
                    }
                }
                _ => {}
            }
        }

        Ok(None)
    }
}

struct DFUHostDeviceMetadata {
    protocol: DFUInterfaceProtocol,
    config_value: u8,
    interface_num: u8,
    functional_descriptor: DFUFunctionalDescriptor,
}

struct DFUHostDeviceEntry {
    device_entry: DeviceEntry,
    metadata: DFUHostDeviceMetadata,
}

impl DFUHostDeviceEntry {
    async fn open(self) -> Result<DFUHostDevice> {
        let mut device = self.device_entry.open().await?;

        println!("Open!");

        // In the case of a keyboard we need to detach all interfaces?
        if device.kernel_driver_active(0)? {
            println!("Removing kernel driver..");
            device.detach_kernel_driver(0)?;
        }

        device.set_active_configuration(self.metadata.config_value)?;

        // TODO: Also set the alternative setting.
        device.claim_interface(self.metadata.interface_num)?;

        Ok(DFUHostDevice {
            device,
            metadata: self.metadata,
        })
    }
}

struct DFUHostDevice {
    device: Device,
    metadata: DFUHostDeviceMetadata,
}

impl DFUHostDevice {
    /// Asks the device to stop what its doing and change its state back to
    /// 'idle'. This also sets the status back to OK.
    async fn abort(&self) -> Result<()> {
        self.device
            .write_control(
                SetupPacket {
                    bmRequestType: 0b00100001,
                    bRequest: DFURequestType::DFU_ABORT as u8,
                    wValue: 0,
                    wIndex: self.metadata.interface_num as u16,
                    wLength: 0,
                },
                &[],
            )
            .await
    }

    async fn download_block(&self, block_num: u16, block_data: &[u8]) -> Result<()> {
        let result = self
            .device
            .write_control(
                SetupPacket {
                    bmRequestType: 0b00100001,
                    bRequest: DFURequestType::DFU_DNLOAD as u8,
                    wValue: block_num,
                    wIndex: self.metadata.interface_num as u16,
                    wLength: block_data.len() as u16,
                },
                block_data,
            )
            .await;

        let stalled = match result {
            Ok(()) => false,
            Err(e) => {
                if let Some(crate::Error::TransferStalled) = e.downcast_ref() {
                    true
                } else {
                    return Err(e);
                }
            }
        };

        let mut status_buffer = [0u8; core::mem::size_of::<DFUStatus>()];

        // NOTE: GET_STATUS should be sent after every DFU_DNLOAD
        self.device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b10100001,
                    bRequest: DFURequestType::DFU_GETSTATUS as u8,
                    wValue: 0,
                    wIndex: self.metadata.interface_num as u16,
                    wLength: status_buffer.len() as u16,
                },
                &mut status_buffer,
            )
            .await?;

        let status: &DFUStatus = unsafe { core::mem::transmute(status_buffer.as_ptr()) };

        if status.bStatus != DFUStatusCode::OK {
            let desc: String = {
                if status.iString != 0 {
                    self.device.read_local_string(status.iString).await?
                } else {
                    status
                        .bStatus
                        .default_description()
                        .unwrap_or("[unknown]")
                        .into()
                }
            };

            self.device
                .write_control(
                    SetupPacket {
                        bmRequestType: 0b00100001,
                        bRequest: DFURequestType::DFU_CLRSTATUS as u8,
                        wValue: 0,
                        wIndex: self.metadata.interface_num as u16,
                        wLength: 0,
                    },
                    &[],
                )
                .await?;

            // TODO: Send a DFU_ABORT here?

            return Err(format_err!(
                "DFU Error: [{}]: {}",
                status.bStatus.name().unwrap_or("unknown"),
                desc
            ));
        } else if stalled {
            return Err(err_msg("DFU_DNLOAD stalled but no error status reported"));
        }

        Ok(())
    }
}
