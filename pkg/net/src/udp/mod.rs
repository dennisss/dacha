mod options;
pub use options::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(target_os = "linux"))]
mod mio;
#[cfg(not(target_os = "linux"))]
pub use mio::*;
