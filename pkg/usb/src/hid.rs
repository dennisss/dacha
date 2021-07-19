
use common::errors::*;

enum_def!(HIDDescriptorType u8 =>
    HID = 0x21,
    Report = 0x22,
    PhysicalDescriptor = 0x23
);

enum_def!(HIDRequestType u8 =>
    GET_REPORT = 0x01,
    GET_IDLE = 0x02,
    GET_PROTOCOL = 0x03,
    SET_REPORT = 0x09,
    SET_IDLE = 0x0A,
    SET_PROTOCOL = 0x0B
);