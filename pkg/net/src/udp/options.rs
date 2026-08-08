use alloc::string::{String, ToString};

// TODO: Most of these settings are linux only
#[derive(Default)]
pub struct UdpBindOptions {
    pub(super) reuse_addr: bool,
    pub(super) reuse_port: bool,
    pub(super) broadcast: bool,

    #[cfg(target_os = "linux")]
    pub(super) bind_to_device: Option<String>,

    #[cfg(target_os = "linux")]
    pub(super) enable_hardware_timestamping: bool,
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

    #[cfg(target_os = "linux")]
    pub fn bind_to_device(&mut self, value: &str) -> &mut Self {
        self.bind_to_device = Some(value.to_string());
        self
    }

    #[cfg(target_os = "linux")]
    pub fn enable_hardware_timestamping(&mut self) -> &mut Self {
        self.enable_hardware_timestamping = true;
        self
    }
}