use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use common::line_builder::LineBuilder;
use core::mem::size_of;
use std::collections::HashMap;

use common::errors::*;
use common::struct_bytes::struct_bytes;

use crate::descriptors::*;

pub struct DescriptorSetBuilder {
    strings: Vec<String>,
}

impl DescriptorSetBuilder {
    pub fn new() -> Self {
        Self { strings: vec![] }
    }

    pub fn add_string(&mut self, value: &str) -> u8 {
        self.strings.push(value.to_owned());
        self.strings.len() as u8
    }

    pub fn with_device(self, mut device: DeviceDescriptor) -> DeviceDescriptorSetBuilder {
        device.bLength = size_of::<DeviceDescriptor>() as u8;
        device.bDescriptorType = DescriptorType::DEVICE as u8;
        device.bNumConfigurations = 0;

        DeviceDescriptorSetBuilder {
            device,
            data: vec![],
            strings: self.strings,
            config_start_indices: vec![],
            next_endpoint_index: 1,
            constants: HashMap::new(),
        }
    }
}

pub struct DeviceDescriptorSetBuilder {
    device: DeviceDescriptor,

    data: Vec<u8>,

    config_start_indices: Vec<usize>,

    strings: Vec<String>,

    // TODO: Support using the same index for both an IN and OUT endpoint.
    next_endpoint_index: usize,

    constants: HashMap<String, usize>,
}

impl DeviceDescriptorSetBuilder {
    pub fn add_config(
        &mut self,
        mut config: ConfigurationDescriptor,
    ) -> ConfigDescriptorSetBuilder {
        self.config_start_indices.push(self.data.len());
        self.device.bNumConfigurations += 1;

        config.bLength = size_of::<ConfigurationDescriptor>() as u8;
        config.bDescriptorType = DescriptorType::CONFIGURATION as u8;
        config.wTotalLength = size_of::<ConfigurationDescriptor>() as u16;
        config.bNumInterfaces = 0;
        config.bConfigurationValue = self.config_start_indices.len() as u8;

        ConfigDescriptorSetBuilder {
            parent: self,
            config,
            data: vec![],
        }
    }

    pub fn add_string(&mut self, value: &str) -> u8 {
        self.strings.push(value.to_owned());
        self.strings.len() as u8
    }

    pub fn generate_code(&self, name: &str) -> Result<String> {
        let mut data = vec![];

        let device_start_index = data.len();
        data.extend_from_slice(unsafe { struct_bytes(&self.device) });
        let device_end_index = data.len();

        data.extend_from_slice(&self.data);

        // End index after all config descriptors.
        let configs_end_index = data.len();

        let mut string_start_indices = vec![];
        let mut string_end_indices = vec![];

        // 0'th string is always just for enumerating languages
        string_start_indices.push(data.len());
        data.extend_from_slice(&[
            4,                            // bLength
            DescriptorType::STRING as u8, // bDescriptorType
            0x09,                         // English
            0x04,                         // US
        ]);
        string_end_indices.push(data.len());

        for s in &self.strings {
            string_start_indices.push(data.len());

            // Encode as little endian UTF-16
            let s_data = {
                let mut buf = vec![];
                for i in s.encode_utf16() {
                    buf.extend_from_slice(&i.to_le_bytes());
                }
                buf
            };

            data.push((2 + s_data.len()) as u8);
            data.push(DescriptorType::STRING as u8);
            data.extend_from_slice(&s_data);

            string_end_indices.push(data.len());
        }

        //////

        let config_cases = {
            let mut lines = LineBuilder::new();

            for i in 0..self.config_start_indices.len() {
                let start = device_end_index + self.config_start_indices[i];
                let end = self
                    .config_start_indices
                    .get(i + 1)
                    .cloned()
                    .unwrap_or(configs_end_index);

                lines.add(format!("{} => Some(&Self::raw()[{}..{}]),", i, start, end));
            }

            lines.to_string()
        };

        let string_cases = {
            let mut lines = LineBuilder::new();

            for i in 0..string_start_indices.len() {
                lines.add(format!(
                    "{} => Some(&Self::raw()[{}..{}]),",
                    i, string_start_indices[i], string_end_indices[i]
                ));
            }

            lines.to_string()
        };

        let mut lines = LineBuilder::new();

        lines.add(format!(
            r#"

            pub static {name_upper_snake}: {name} = {name} {{ hidden: () }};

            #[derive(Clone, Copy)]
            pub struct {name} {{
                hidden: ()
            }}

            impl {name} {{
                fn raw() -> &'static [u8] {{
                    static DATA: &'static [u8] = &{raw_data:?};
                    DATA
                }}
            }}

            impl ::usb::DescriptorSet for {name} {{
                fn device_bytes(&self) -> &[u8] {{
                    &Self::raw()[{device_start_index}..{device_end_index}]
                }}

                fn config_bytes(&self, index: u8) -> Option<&[u8]> {{
                    match index {{
                        {config_cases}
                        _ => None
                    }}
                }}

                fn string_bytes(&self, index: u8) -> Option<&[u8]> {{
                    match index {{
                        {string_cases}
                        _ => None
                    }}
                }}
            }}
        "#,
            name = name,
            raw_data = data,
            device_start_index = device_start_index,
            device_end_index = device_end_index,
            config_cases = config_cases,
            string_cases = string_cases,
            name_upper_snake = common::camel_to_snake_case(name).to_ascii_uppercase()
        ));

        for (tag, value) in &self.constants {
            lines.add(format!(
                r#"
                impl ::common::attribute::GetAttributeValue<{tag}> for {name} {{
                    fn get_attr_value(&self) -> u8 {{
                        {value}
                    }}
                }}
            "#,
                name = name,
                tag = tag,
                value = value
            ));
        }

        Ok(lines.to_string())
    }
}

pub struct ConfigDescriptorSetBuilder<'a> {
    parent: &'a mut DeviceDescriptorSetBuilder,
    config: ConfigurationDescriptor,
    data: Vec<u8>,
}

impl<'a> Drop for ConfigDescriptorSetBuilder<'a> {
    fn drop(&mut self) {
        self.config.wTotalLength = (size_of::<ConfigurationDescriptor>() + self.data.len()) as u16;

        self.parent
            .data
            .extend_from_slice(unsafe { struct_bytes(&self.config) });
        self.parent.data.extend_from_slice(&self.data);
    }
}

impl<'a> ConfigDescriptorSetBuilder<'a> {
    pub fn add_interface<'b>(
        &'b mut self,
        name: &str,
        mut iface: InterfaceDescriptor,
    ) -> InterfaceDescriptorSetBuilder<'a, 'b> {
        iface.bLength = size_of::<InterfaceDescriptor>() as u8;
        iface.bDescriptorType = DescriptorType::INTERFACE as u8;
        iface.bInterfaceNumber = self.config.bNumInterfaces;
        self.config.bNumInterfaces += 1;

        assert_eq!(iface.bAlternateSetting, 0);

        iface.bNumEndpoints = 0;

        if !name.is_empty() {
            self.parent
                .constants
                .insert(format!("{}", name), iface.bInterfaceNumber as usize);
        }

        InterfaceDescriptorSetBuilder {
            parent: self,
            iface,
            data: vec![],
        }
    }

    pub fn add_string(&mut self, value: &str) -> u8 {
        self.parent.add_string(value)
    }
}

pub struct InterfaceDescriptorSetBuilder<'a, 'b> {
    parent: &'b mut ConfigDescriptorSetBuilder<'a>,
    iface: InterfaceDescriptor,
    data: Vec<u8>,
}

impl<'a, 'b> Drop for InterfaceDescriptorSetBuilder<'a, 'b> {
    fn drop(&mut self) {
        self.parent
            .data
            .extend_from_slice(unsafe { struct_bytes(&self.iface) });
        self.parent.data.extend_from_slice(&self.data);
    }
}

impl<'a, 'b> InterfaceDescriptorSetBuilder<'a, 'b> {
    // TODO: I need to know the new enpoitn index for code gen.
    pub fn add_endpoint(&mut self, name: &str, mut endpoint: EndpointDescriptor) -> &mut Self {
        self.iface.bNumEndpoints += 1;

        endpoint.bLength = size_of::<EndpointDescriptor>() as u8;
        endpoint.bDescriptorType = DescriptorType::ENDPOINT as u8;
        endpoint.bEndpointAddress =
            (endpoint.bEndpointAddress & (1 << 7)) | (self.parent.parent.next_endpoint_index as u8);
        self.parent.parent.next_endpoint_index += 1;

        self.data
            .extend_from_slice(unsafe { struct_bytes(&endpoint) });

        // TODO: Verify there are no conflicting entries.
        if !name.is_empty() {
            self.parent
                .parent
                .constants
                .insert(format!("{}", name), endpoint.bEndpointAddress as usize);
        }

        self
    }

    pub fn add_generic_descriptor<T>(&mut self, desc: T) -> &mut Self {
        self.data.extend_from_slice(unsafe { struct_bytes(&desc) });

        self
    }

    pub fn add_string(&mut self, value: &str) -> u8 {
        self.parent.add_string(value)
    }
}
