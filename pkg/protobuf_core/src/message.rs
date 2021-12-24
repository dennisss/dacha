#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;

use common::errors::*;
use common::list::Appendable;

#[cfg(feature = "alloc")]
use crate::merge::ReflectMergeFrom;
use crate::types::EnumValue;
#[cfg(feature = "alloc")]
use crate::{MessageReflection, StaticFileDescriptor};

#[cfg(feature = "alloc")]
pub trait MessageTraits = Send + Sync + MessageReflection;
#[cfg(not(feature = "alloc"))]
pub trait MessageTraits = Send + Sync;

// NOTE: Construct an empty proto by calling MessageType::default()
// Clone + std::fmt::Debug + std::default::Default + MessageReflection
pub trait Message: 'static + MessageTraits {
    fn type_url(&self) -> &'static str;

    #[cfg(feature = "alloc")]
    fn file_descriptor() -> &'static StaticFileDescriptor
    where
        Self: Sized;

    // NOTE: This will append values to
    fn parse(data: &[u8]) -> Result<Self>
    where
        Self: Sized;

    fn parse_merge(&mut self, data: &[u8]) -> Result<()>;

    /// Serializes the protobuf as a vector.
    #[cfg(feature = "alloc")]
    fn serialize(&self) -> Result<Vec<u8>> {
        let mut data = vec![];
        self.serialize_to(&mut data)?;
        Ok(data)
    }

    fn serialize_to<A: Appendable<Item = u8>>(&self, out: &mut A) -> Result<()>;

    // TODO: Add serialize_to with Appendable.

    // TODO: should be a shared reference?
    // fn descriptor() -> Descriptor;

    // TODO: Serializers must return a result because required conditions may
    // not be satisfied.

    #[cfg(feature = "alloc")]
    fn merge_from(&mut self, other: &Self) -> Result<()>
    where
        Self: Sized,
    {
        self.reflect_merge_from(other)
    }

    // fn unknown_fields() -> &[UnknownField];
}

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(Fail))]
#[repr(u32)]
pub enum MessageParseError {
    UnknownEnumVariant,
}

impl core::fmt::Display for MessageParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(Fail))]
#[repr(u32)]
pub enum MessageSerializeError {
    RequiredFieldNotSet,
}

impl core::fmt::Display for MessageSerializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A pointer to a Message. Used in message fields to support storing possibly
/// recursive type usages.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct MessagePtr<T> {
    #[cfg(feature = "alloc")]
    value: Box<T>,
    #[cfg(not(feature = "alloc"))]
    value: T,
}

impl<T> MessagePtr<T> {
    pub fn new(value: T) -> Self {
        Self {
            #[cfg(feature = "alloc")]
            value: Box::new(value),
            #[cfg(not(feature = "alloc"))]
            value,
        }
    }
}

impl<T> core::ops::Deref for MessagePtr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> core::ops::DerefMut for MessagePtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> core::convert::AsRef<T> for MessagePtr<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> core::convert::AsMut<T> for MessagePtr<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Common trait implemented by all code generated protobuf enum types.
pub trait Enum {
    /// Should convert a number to a valid branch of the enum, or else should
    /// error out it the value is not in the enum.
    fn parse(v: EnumValue) -> Result<Self>
    where
        Self: Sized;

    fn parse_name(name: &str) -> Result<Self>
    where
        Self: Sized;

    fn name(&self) -> &str;
    fn value(&self) -> EnumValue;

    fn assign(&mut self, v: EnumValue) -> Result<()>;
    // TODO: This is inconsistent with the other Message trait.

    fn assign_name(&mut self, name: &str) -> Result<()>;
}
