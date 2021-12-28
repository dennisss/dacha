use common::struct_bytes::struct_bytes;
use usb::descriptors::*;

#[repr(packed)]
pub struct Descriptors {
    pub device: DeviceDescriptor,
    pub config: ConfigurationDescriptor,
    pub iface: InterfaceDescriptor,
    pub ep1: EndpointDescriptor,
    pub ep2: EndpointDescriptor,
}

impl Descriptors {
    pub fn device_bytes(&self) -> &[u8] {
        unsafe { struct_bytes(&self.device) }
    }

    pub fn config_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                core::mem::transmute(&self.config),
                self.config.wTotalLength as usize,
            )
        }
    }

    // TODO: Implement reading by a given index
    pub fn endpoint_bytes(&self, index: usize) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(core::mem::transmute(&self.ep1), self.ep1.bLength as usize)
        }
    }
}

pub static DESCRIPTORS: Descriptors = Descriptors {
    device: DeviceDescriptor {
        bLength: core::mem::size_of::<DeviceDescriptor>() as u8,
        bDescriptorType: DescriptorType::DEVICE as u8,
        bcdUSB: 0x0200, // 2.0
        bDeviceClass: 0,
        bDeviceSubClass: 0,
        bDeviceProtocol: 0,
        bMaxPacketSize0: 64,
        idVendor: 0x8888,
        idProduct: 0x0001,
        bcdDevice: 0x0100, // 1.0,
        iManufacturer: 1,
        iProduct: 2,
        iSerialNumber: 0,
        bNumConfigurations: 1,
    },
    config: ConfigurationDescriptor {
        bLength: core::mem::size_of::<ConfigurationDescriptor>() as u8,
        bDescriptorType: DescriptorType::CONFIGURATION as u8,
        // TODO: Make this field more maintainable.
        wTotalLength: (core::mem::size_of::<ConfigurationDescriptor>()
            + core::mem::size_of::<InterfaceDescriptor>()
            + 2 * core::mem::size_of::<EndpointDescriptor>()) as u16,
        bNumInterfaces: 1,
        bConfigurationValue: 1,
        iConfiguration: 0,
        // TODO: Double check this
        bmAttributes: 0xa0, // Bus Powered : Remote wakeup
        bMaxPower: 50,
    },
    iface: InterfaceDescriptor {
        bLength: core::mem::size_of::<InterfaceDescriptor>() as u8,
        bDescriptorType: DescriptorType::INTERFACE as u8,
        bInterfaceNumber: 0,
        bAlternateSetting: 0,
        bNumEndpoints: 2,
        bInterfaceClass: 0, // TODO
        bInterfaceSubClass: 0,
        bInterfaceProtocol: 0,
        iInterface: 0,
    },
    ep1: EndpointDescriptor {
        bLength: core::mem::size_of::<EndpointDescriptor>() as u8,
        bDescriptorType: DescriptorType::ENDPOINT as u8,
        bEndpointAddress: 0x81, // EP IN 1
        bmAttributes: 0b11,     // Interrupt
        wMaxPacketSize: 64,
        bInterval: 64, // TODO: Check me.
    },
    ep2: EndpointDescriptor {
        bLength: core::mem::size_of::<EndpointDescriptor>() as u8,
        bDescriptorType: DescriptorType::ENDPOINT as u8,
        bEndpointAddress: 0x02, // EP OUT 2
        bmAttributes: 0b11,     // Interrupt
        wMaxPacketSize: 64,
        bInterval: 64, // TODO: Check me.
    },
};

pub static STRING_DESC0: &'static [u8] = &[
    4,                            // bLength
    DescriptorType::STRING as u8, // bDescriptorType
    0x09,                         // English
    0x04,                         // US
];

pub static STRING_DESC1: &'static [u8] =
    &[8, DescriptorType::STRING as u8, b'd', 0, b'a', 0, b'!', 0];
