#![feature(core_intrinsics, trait_alias)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

#[cfg(feature = "std")]
#[macro_use]
extern crate parsing;

#[cfg(feature = "alloc")]
mod bytes;
#[cfg(feature = "std")]
mod collections;
#[cfg(feature = "alloc")]
mod merge;
mod message;
#[cfg(feature = "alloc")]
pub mod reflection;
#[cfg(feature = "std")]
pub mod text;
#[cfg(feature = "std")]
pub mod tokenizer;
mod types;
pub mod wire;

#[cfg(feature = "alloc")]
pub use bytes::BytesField;
#[cfg(feature = "std")]
pub use collections::*;
pub use message::{Enum, Message, MessageParseError, MessagePtr, MessageSerializeError};
#[cfg(feature = "std")]
pub use reflection::{
    FieldDescriptorShort, MessageReflection, SingularFieldReflectionProto2,
    SingularFieldReflectionProto3, StringPtr,
};
pub use types::EnumValue;
pub use types::FieldNumber;

pub struct StaticFileDescriptor {
    pub proto: &'static [u8],
    pub dependencies: &'static [&'static StaticFileDescriptor],
}
