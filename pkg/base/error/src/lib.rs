#![feature(trait_alias)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
pub extern crate failure;

#[cfg(feature = "std")]
mod error_failure;
#[cfg(feature = "std")]
pub use error_failure::*;

pub mod error_new;
#[cfg(not(feature = "std"))]
pub use error_new::*;

#[cfg(feature = "std")]
mod helpers;
#[cfg(feature = "std")]
pub use helpers::*;

#[cfg(feature = "std")]
mod latching;
#[cfg(feature = "std")]
pub use latching::*;

pub trait TryIntoResult<T> {
    fn try_into_result(self) -> Result<T>;
}

impl<T> TryIntoResult<T> for T {
    fn try_into_result(self) -> Result<T> {
        Ok(self)
    }
}
