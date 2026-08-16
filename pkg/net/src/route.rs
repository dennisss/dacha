use alloc::string::String;

use crate::ip::IPAddress;

#[derive(Clone, Debug)]
pub struct NetworkInterfaceRoute {
    /// Name of the network interface.
    pub name: String,

    /// Local address of the interface
    pub addr: IPAddress,

    /// Index of the network interface.
    pub index: u32,
}