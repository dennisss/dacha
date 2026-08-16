use alloc::string::{String, ToString};

use crate::ip::SocketAddr;
use crate::route::NetworkInterfaceRoute;
use crate::socket::SocketOptions;

#[derive(Default)]
pub struct TcpConnectOptions {
    pub(crate) inner: SocketOptions,
}

impl TcpConnectOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_addr(&mut self, addr: SocketAddr) -> &mut Self {
        self.inner.bind_addr = Some(addr);
        self
    } 

    pub fn bind_to_device(&mut self, value: &str) -> &mut Self {
        self.inner.bind_to_device = Some(value.to_string());
        self
    }

    // Implies bind_addr, bind_to_device
    pub fn route(&mut self, route: NetworkInterfaceRoute) -> &mut Self {
        self.inner.bind_to_device = Some(route.name);
        self.inner.bind_addr = Some(SocketAddr::new(route.addr, 0)); // any port
        self.inner.device_index = Some(route.index);
        self
    }
}