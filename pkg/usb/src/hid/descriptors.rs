use std::collections::{HashMap, HashSet};

use common::async_std::task::current;
use common::errors::*;

use crate::descriptors::{SetupPacket, StandardRequestType};
use crate::endpoint::is_in_endpoint;
use crate::linux::Device;
use crate::descriptor_iter::Descriptor;


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

#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct HIDDescriptor {
    pub bLength: u8,
    pub bDescriptorType: u8,
    pub bcdHID: u16,
    pub bCountryCode: u8,
    pub bNumDescriptors: u8,
    pub bReportDescriptorType: u8,
    pub wReportDescriptorLength: u16
}


