mod descriptors;

#[cfg(feature = "alloc")]
mod desciptor_builders;

#[cfg(feature = "std")]
mod host;
#[cfg(feature = "std")]
pub use host::*;

#[cfg(feature = "alloc")]
pub use desciptor_builders::*;
pub use descriptors::*;

define_attr!(DFUInterfaceNumberTag => u8);
