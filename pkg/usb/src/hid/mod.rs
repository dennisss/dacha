mod country_code;
mod descriptors;
#[cfg(feature = "std")]
mod device;
#[cfg(feature = "alloc")]
mod item;
mod keyboard_report;
#[cfg(feature = "alloc")]
mod keyboard_report_descriptor;

pub use country_code::*;
pub use descriptors::*;
#[cfg(feature = "std")]
pub use device::HIDDevice;
#[cfg(feature = "alloc")]
pub use item::ReportType;
#[cfg(feature = "alloc")]
pub use item::*;
pub use keyboard_report::*;
#[cfg(feature = "alloc")]
pub use keyboard_report_descriptor::*;

define_attr!(HIDInterfaceNumberTag => u8);
define_attr!(HIDInterruptInEndpointTag => u8);
