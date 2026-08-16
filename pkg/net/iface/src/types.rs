use net::ip::*;

#[derive(Clone, Debug)]
pub struct NetworkInterface {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub typ: NetworkInterfaceType,
    pub addrs: Vec<NetworkInterfaceAddrs>,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkInterfaceType {
    Unknown,
    PhysicalEthernet,
    PhysicalWireless,
    Tunnel,
    Loopback,
}

#[derive(Clone, Debug)]
pub struct NetworkInterfaceAddrs {
    pub addr: NetworkInterfaceAddr,
    pub netmask: Option<NetworkInterfaceAddr>,
    // pub broadcast
}

#[derive(Clone, Debug)]
pub enum NetworkInterfaceAddr {
    IP(IPAddress),
    Link,
    Link2(Vec<u8>),
    Unknown
}