use alloc::string::String;

use common::args::{ArgFieldType, ArgType, ArgsType, RawArgs};
use common::errors::*;

use crate::DeviceEntry;

regexp!(DEVICE_ID_PATTERN => "^([0-9a-fA-F]{4}):([0-9a-fA-F]{4})?$");

regexp!(DEVICE_NUM_PATTERN => "^([0-9]+).([0-9]+)$");

#[derive(Default)]
pub struct DeviceSelector {
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub bus_num: Option<usize>,
    pub device_num: Option<usize>,
}

impl DeviceSelector {
    pub fn matches(&self, device_entry: &DeviceEntry) -> Result<bool> {
        if let Some(num) = self.bus_num {
            if num != device_entry.bus_num() {
                return Ok(false);
            }
        }

        if let Some(num) = self.device_num {
            if num != device_entry.dev_num() {
                return Ok(false);
            }
        }

        let device_desc = device_entry.device_descriptor()?;

        if let Some(id) = self.vendor_id {
            if id != device_desc.idVendor {
                return Ok(false);
            }
        }

        if let Some(id) = self.product_id {
            if id != device_desc.idProduct {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

impl ArgsType for DeviceSelector {
    fn parse_raw_args(raw_args: &mut RawArgs) -> Result<Self>
    where
        Self: Sized,
    {
        let raw = DeviceSelectorArgs::parse_raw_args(raw_args)?;

        let mut vendor_id = None;
        let mut product_id = None;
        let mut bus_num = None;
        let mut device_num = None;

        if let Some(id) = raw.usb_device_id {
            let m = DEVICE_ID_PATTERN
                .exec(id.as_str())
                .ok_or_else(|| err_msg("Invalid usb_device_id"))?;

            vendor_id = Some(u16::from_str_radix(m.group_str(1).unwrap()?, 16)?);
            if let Some(val) = m.group_str(2) {
                product_id = Some(u16::from_str_radix(val?, 16)?);
            }
        }

        if let Some(num) = raw.usb_device_num {
            let m = DEVICE_NUM_PATTERN
                .exec(num.as_str())
                .ok_or_else(|| err_msg("Invalid usb_device_num"))?;

            bus_num = Some(m.group_str(1).unwrap()?.parse()?);
            device_num = Some(m.group_str(1).unwrap()?.parse()?);
        }

        Ok(Self {
            vendor_id,
            product_id,
            bus_num,
            device_num,
        })
    }
}

impl ArgFieldType for DeviceSelector {
    fn parse_raw_arg_field(
        field_name: &str,
        raw_args: &mut ::common::args::RawArgs,
    ) -> Result<Self> {
        // NOTE: The field_name is ignored.
        Self::parse_raw_args(raw_args)
    }
}

#[derive(Args)]
struct DeviceSelectorArgs {
    usb_device_id: Option<String>,

    usb_device_num: Option<String>,
    // usb_device_port: Option<String>
}
