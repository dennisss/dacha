#![allow(non_camel_case_types, non_snake_case)]

// NOTE: 0 means a null string reference.
// NOTE: All fields are little endian

// Figure 8-15 shows a Data Packet (which has a CRC 16)

#[derive(Default)]
#[repr(packed)]
pub struct SetupPacket {
    pub bmRequestType: u8,
    pub bRequest: u8,
    pub wValue: u16,
    pub wIndex: u16,
    pub wLength: u16,
}

// Table 9-4 of USB2.0 Spec
pub enum StandardRequestType {
    GET_STATUS = 0,
    CLEAR_FEATURE = 1,
    SET_FEATURE = 3,
    SET_ADDRESS = 5,
    GET_DESCRIPTOR = 6,
    SET_DESCRIPTOR = 7,
    GET_CONFIGURATION = 8,
    SET_CONFIGURATION = 9,
    GET_INTERFACE = 10,
    SET_INTERFACE = 11,
    SYNCH_FRAME = 12,
}

// Table 9-5 of USB2.0 Spec
#[derive(Debug, PartialEq)]
pub enum DescriptorType {
    DEVICE = 1,
    CONFIGURATION = 2,
    STRING = 3,
    INTERFACE = 4,
    ENDPOINT = 5,
    DEVICE_QUALIFIER = 6,
    OTHER_SPEED_CONFIGURATION = 7,
    INTERFACE_POWER1 = 8,
}

impl DescriptorType {
    pub fn from_value(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::DEVICE,
            2 => Self::CONFIGURATION,
            3 => Self::STRING,
            4 => Self::INTERFACE,
            5 => Self::ENDPOINT,
            6 => Self::DEVICE_QUALIFIER,
            7 => Self::OTHER_SPEED_CONFIGURATION,
            8 => Self::INTERFACE_POWER1,
            _ => {
                return None;
            }
        })
    }
}

// Table 9-8 of USB2.0 Spec
#[derive(Clone, Copy)]
#[repr(packed)]
pub struct DeviceDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub bcdUSB: u16,
    pub bDeviceClass: u8,
    pub bDeviceSubClass: u8,
    pub bDeviceProtocol: u8,
    pub bMaxPacketSize0: u8,
    pub idVendor: u16,
    pub idProduct: u16,
    pub bcdDevice: u16,
    pub iManufacturer: u8,
    pub iProduct: u8,
    pub iSerialNumber: u8,
    pub bNumConfigurations: u8,
}

// Table 9-9 of USB2.0 Spec
#[repr(packed)]
pub struct DeviceQualifierDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub bcdUSB: u16,
    pub bDeviceClass: u8,
    pub bDeviceSubClass: u8,
    pub bDeviceProtocol: u8,
    pub bMaxPacketSize0: u8,
    pub bNumConfigurations: u8,
    pub bReserved: u8,
}

// Table 9-10 of USB2.0 Spec
#[derive(Clone, Copy)]
#[repr(packed)]
pub struct ConfigurationDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub wTotalLength: u16,
    pub bNumInterfaces: u8,
    pub bConfigurationValue: u8,
    pub iConfiguration: u8,
    pub bmAttributes: u8,
    pub bMaxPower: u8,
}

// Table 9-12 of USB2.0 Spec
#[derive(Clone, Copy)]
#[repr(packed)]
pub struct InterfaceDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub bInterfaceNumber: u8,
    pub bAlternateSetting: u8,
    pub bNumEndpoints: u8,
    pub bInterfaceClass: u8,
    pub bInterfaceSubClass: u8,
    pub bInterfaceProtocol: u8,
    pub iInterface: u8,
}

// Table 9-13 of USB2.0 Spec
#[derive(Clone, Copy)]
#[repr(packed)]
pub struct EndpointDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub bEndpointAddress: u8,
    pub bmAttributes: u8,
    pub wMaxPacketSize: u16,
    pub bInterval: u8,
}

pub enum USBLangId {
    EnglishUS = 0x0409,
}

// Table 9-16 of USB2.0 Spec
// NOTE: Special care must be taken to serialize or de-serialize this descriptor
pub struct StringDescriptor<'a> {
    bLength: u8,
    bDescriptorType: u8,
    bString: &'a [u8],
}
