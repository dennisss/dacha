use crate::ip::IPAddress;

pub const MAX_PACKET_SIZE: usize = 512;

pub const DEFAULT_PORT: u16 = 53;

pub const MULTICAST_ADDR: IPAddress = IPAddress::V4([224, 0, 0, 251]);

pub const MULTICAST_PORT: u16 = 5353;