use common::errors::*;

use crate::hid::{HIDDescriptor, HIDDescriptorType};
use crate::descriptors::*;

pub enum Descriptor {
    Device(DeviceDescriptor),
    Configuration(ConfigurationDescriptor),
    Endpoint(EndpointDescriptor),
    Interface(InterfaceDescriptor),
    // String(StringDescriptor),

    HID(HIDDescriptor),

    Unknown(Vec<u8>),
}

/// Iterates over a list of concatenated USB descriptors in binary form.
pub struct DescriptorIter<'a> {
    data: &'a [u8],
    in_hid_interface: bool
}

impl<'a> DescriptorIter<'a> {
    pub fn new(data: &[u8]) -> DescriptorIter {
        DescriptorIter { data, in_hid_interface: false }
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

        fn decode_fixed_len_desc<T: Copy>(raw_desc: &[u8]) -> Result<T> {
            if raw_desc.len() != std::mem::size_of::<T>() {
                return Err(err_msg("Descriptor is the wrong size"));
            }

            // TODO: This transmute assumes that we are running on a little-endian system
            // (same as the wire endian of the USB descriptors).
            Ok(*unsafe { std::mem::transmute::<_, &T>(raw_desc.as_ptr()) })
        }

        let in_hid_interface = self.in_hid_interface;
        // self.in_hid_interface = false;

        Ok(Some(match typ {
            Some(DescriptorType::DEVICE) => Descriptor::Device(decode_fixed_len_desc(raw_desc)?),
            Some(DescriptorType::CONFIGURATION) => {
                Descriptor::Configuration(decode_fixed_len_desc(raw_desc)?)
            }
            Some(DescriptorType::ENDPOINT) => {
                Descriptor::Endpoint(decode_fixed_len_desc(raw_desc)?)
            }
            Some(DescriptorType::INTERFACE) => {
                let iface: InterfaceDescriptor = decode_fixed_len_desc(raw_desc)?;
                self.in_hid_interface = iface.bInterfaceClass == InterfaceClass::HID as u8;
                Descriptor::Interface(iface)
            }
            _ => {
                if in_hid_interface {
                    if raw_type == HIDDescriptorType::HID as u8 {
                        let hid = decode_fixed_len_desc(raw_desc)?;
                        return Ok(Some(Descriptor::HID(hid)));
                    }
                }

                // TODO: Support all the types supported by linux. See:
                // https://github.com/torvalds/linux/blob/master/include/uapi/linux/usb/ch9.h
                Descriptor::Unknown(raw_desc.to_vec())
            }
        }))
    }
}

impl<'a> std::iter::Iterator for DescriptorIter<'a> {
    type Item = Result<Descriptor>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_impl() {
            Ok(v) => v.map(|v| Ok(v)),
            Err(e) => Some(Err(e)),
        }
    }
}