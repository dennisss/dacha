#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;

mod bytes;
mod collections;
mod message;
pub mod reflection;
mod types;
pub mod wire;

pub use bytes::BytesField;
pub use collections::*;
pub use message::{Enum, Message, MessagePtr};
pub use reflection::{FieldDescriptor, MessageReflection};
pub use types::EnumValue;
pub use types::FieldNumber;

pub struct StaticFileDescriptor {
    pub proto: &'static [u8],
    pub dependencies: &'static [&'static StaticFileDescriptor],
}
