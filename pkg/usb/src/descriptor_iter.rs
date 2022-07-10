use alloc::vec::Vec;
use core::iter::Iterator;

use common::errors::*;

use crate::descriptors::*;
use crate::dfu::DFU_INTERFACE_SUBCLASS;
use crate::hid::{HIDDescriptor, HIDDescriptorType};

#[derive(Debug)]
pub enum Descriptor {
    Device(DeviceDescriptor),
    Configuration(ConfigurationDescriptor),
    Endpoint(EndpointDescriptor),
    Interface(InterfaceDescriptor),
    HID(HIDDescriptor),

    Unknown(UnknownDescriptor<Vec<u8>>),
}

#[derive(Debug)]
pub struct UnknownDescriptor<D> {
    data: D,
}

impl<D: AsRef<[u8]>> UnknownDescriptor<D> {
    fn new(data: D) -> Self {
        Self { data }
    }

    pub fn decode<T: Copy>(&self) -> Result<T> {
        let data = self.data.as_ref();

        if data.len() != core::mem::size_of::<T>() {
            return Err(format_err!(
                "Descriptor is the wrong size: {} vs {}",
                data.len(),
                core::mem::size_of::<T>()
            ));
        }

        // TODO: This transmute assumes that we are running on a little-endian system
        // (same as the wire endian of the USB descriptors).
        Ok(*unsafe { core::mem::transmute::<_, &T>(data.as_ptr()) })
    }

    pub fn raw_type(&self) -> u8 {
        self.data.as_ref()[1]
    }
}

/// Iterates over a list of concatenated USB descriptors in binary form.
pub struct DescriptorIter<'a> {
    data: &'a [u8],
    in_hid_interface: bool,
}

impl<'a> DescriptorIter<'a> {
    pub fn new(data: &[u8]) -> DescriptorIter {
        DescriptorIter {
            data,
            in_hid_interface: false,
        }
    }

    fn next_impl(&mut self) -> Result<Option<Descriptor>> {
        if self.data.len() == 0 {
            return Ok(None);
        }

        if self.data.len() < 2 {
            return Err(err_msg("Descriptor too short"));
        }

        // First two bytes of all descriptor types are the same.
        let len = self.data[0] as usize;
        let raw_type = self.data[1];
        let typ = DescriptorType::from_value(raw_type);

        if self.data.len() < len {
            return Err(err_msg("Descriptor overflows buffer"));
        }

        let raw_desc = &self.data[0..len];
        self.data = &self.data[len..];

        let in_hid_interface = self.in_hid_interface;
        // self.in_hid_interface = false;

        Ok(Some(match typ {
            Some(DescriptorType::DEVICE) => {
                Descriptor::Device(UnknownDescriptor::new(raw_desc).decode()?)
            }
            Some(DescriptorType::CONFIGURATION) => {
                Descriptor::Configuration(UnknownDescriptor::new(raw_desc).decode()?)
            }
            Some(DescriptorType::ENDPOINT) => {
                Descriptor::Endpoint(UnknownDescriptor::new(raw_desc).decode()?)
            }
            Some(DescriptorType::INTERFACE) => {
                let iface: InterfaceDescriptor = UnknownDescriptor::new(raw_desc).decode()?;
                self.in_hid_interface = iface.bInterfaceClass == InterfaceClass::HID.to_value();
                Descriptor::Interface(iface)
            }
            _ => {
                if in_hid_interface {
                    if raw_type == HIDDescriptorType::HID as u8 {
                        let hid = UnknownDescriptor::new(raw_desc).decode()?;
                        return Ok(Some(Descriptor::HID(hid)));
                    }
                }

                // TODO: Support all the types supported by linux. See:
                // https://github.com/torvalds/linux/blob/master/include/uapi/linux/usb/ch9.h
                Descriptor::Unknown(UnknownDescriptor::new(raw_desc.to_vec()))
            }
        }))
    }
}

impl<'a> Iterator for DescriptorIter<'a> {
    type Item = Result<Descriptor>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_impl() {
            Ok(v) => v.map(|v| Ok(v)),
            Err(e) => Some(Err(e)),
        }
    }
}
