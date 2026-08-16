use alloc::string::{String, ToString};

use crate::socket::SocketOptions;
use crate::route::*;

// TODO: Most of these settings are linux only
#[derive(Default)]
pub struct UdpBindOptions {
    pub(super) reuse_addr: bool,
    pub(super) reuse_port: bool,
    pub(super) broadcast: bool,

    #[cfg(target_os = "linux")]
    pub(super) enable_hardware_timestamping: bool,

    pub(super) inner: SocketOptions,
}

impl UdpBindOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reuse_addr(&mut self, value: bool) -> &mut Self {
        self.reuse_addr = value;
        self
    }

    pub fn reuse_port(&mut self, value: bool) -> &mut Self {
        self.reuse_port = value;
        self
    }

    pub fn broadcast(&mut self, value: bool) -> &mut Self {
        self.broadcast = value;
        self
    }

    pub fn bind_to_device(&mut self, value: &str) -> &mut Self {
        self.inner.bind_to_device = Some(value.to_string());
        self
    }

    // Implies bind_addr, bind_to_device
    pub fn route(&mut self, route: NetworkInterfaceRoute) -> &mut Self {
        self.inner.bind_to_device = Some(route.name);
        self.inner.device_index = Some(route.index);
        self
    }

    #[cfg(target_os = "linux")]
    pub fn enable_hardware_timestamping(&mut self) -> &mut Self {
        self.enable_hardware_timestamping = true;
        self
    }
}