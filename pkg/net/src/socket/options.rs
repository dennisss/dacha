use alloc::string::String;

use crate::ip::SocketAddr;

#[derive(Clone, Default)]
pub struct SocketOptions {
    pub typ: Option<SocketType>,
    pub bind_addr: Option<SocketAddr>,
    pub bind_to_device: Option<String>,
    pub device_index: Option<u32>,
    pub connect_addr: Option<SocketAddr>,
}

#[derive(Clone, Copy)]
pub enum SocketType {
    UDP,
    TCP
}