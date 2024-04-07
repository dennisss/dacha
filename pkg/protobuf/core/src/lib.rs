#![feature(core_intrinsics, trait_alias, concat_idents)]
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
pub mod codecs;
#[cfg(feature = "std")]
mod collections;
#[cfg(feature = "std")]
pub mod extension;
#[cfg(feature = "alloc")]
mod merge;
mod message;
#[cfg(feature = "std")]
pub mod message_factory;
#[cfg(feature = "alloc")]
pub mod reflection;
#[cfg(feature = "std")]
pub mod text;
#[cfg(feature = "std")]
pub mod tokenizer;
mod types;
#[cfg(feature = "std")]
pub mod unknown;
#[cfg(feature = "std")]
mod value;
pub mod wire;

#[cfg(feature = "alloc")]
pub use bytes::BytesField;
#[cfg(feature = "std")]
pub use collections::*;
pub use extension::{ExtensionRef, ExtensionSet, ExtensionTag};
#[cfg(feature = "alloc")]
pub use merge::*;
pub use message::{Enum, Message, MessagePtr, MessageSerializeError, StaticMessage};
#[cfg(feature = "std")]
pub use reflection::{
    FieldDescriptorShort, MessageReflection, SingularFieldReflectionProto2,
    SingularFieldReflectionProto3, StringPtr,
};
pub use types::FieldNumber;
pub use types::{EnumValue, ExtensionNumberType};
pub use unknown::UnknownFieldSet;
#[cfg(feature = "std")]
pub use value::*;
pub use wire::{WireError, WireResult};

pub struct StaticFileDescriptor {
    /// Serialized FileDescriptorProto.
    pub proto: &'static [u8],

    pub dependencies: &'static [&'static StaticFileDescriptor],
}

pub const TYPE_URL_PREFIX: &'static str = "type.googleapis.com/";
