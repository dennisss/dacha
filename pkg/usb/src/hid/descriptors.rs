enum_def_with_unknown!(
/// Valid values for an HID interface's bInterfaceSubClass field.
HIDInterfaceSubClass u8 =>
    None = 0,

    // This device supports a standard protocol usable by a system's BIOS.
    Boot = 1
);

enum_def_with_unknown!(
/// Valid values for the HID interface's bInterfaceProtocol when the bInterfaceSubClass == Boot
HIDInterfaceBootProtocol u8 =>
    None = 0,
    Keyboard = 1,
    Mouse = 2
);

// Types of descriptors present in an HID interface (based on the
// bInterfaceClass)
enum_def_with_unknown!(HIDDescriptorType u8 =>
    HID = 0x21,
    Report = 0x22,
    PhysicalDescriptor = 0x23
);

enum_def_with_unknown!(HIDRequestType u8 =>
    GET_REPORT = 0x01,
    GET_IDLE = 0x02,
    GET_PROTOCOL = 0x03,
    SET_REPORT = 0x09,
    SET_IDLE = 0x0A,
    SET_PROTOCOL = 0x0B
);

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct HIDDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub bcdHID: u16,
    pub bCountryCode: u8,
    pub bNumDescriptors: u8,
    pub bReportDescriptorType: u8,
    pub wReportDescriptorLength: u16,
}
