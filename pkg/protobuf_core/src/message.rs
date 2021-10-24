use common::errors::*;

use crate::types::EnumValue;
use crate::StaticFileDescriptor;

// NOTE: Construct an empty proto by calling MessageType::default()
// Clone + std::fmt::Debug + std::default::Default + MessageReflection
pub trait Message: 'static + Send + Sync {
    fn type_url(&self) -> &'static str;

    fn file_descriptor() -> &'static StaticFileDescriptor
    where
        Self: Sized;

    // NOTE: This will append values to
    fn parse(data: &[u8]) -> Result<Self>
    where
        Self: Sized;

    fn parse_merge(&mut self, data: &[u8]) -> Result<()>;

    /// Serializes the protobuf as a
    fn serialize(&self) -> Result<Vec<u8>>;

    // TODO: should be a shared reference?
    // fn descriptor() -> Descriptor;

    //	fn parse_text(data: &str) -> Result<Self>;

    // TODO: Must also be able to parse a text proto

    // TODO: Serializers must return a result because required conditions may
    // not be satisfied.

    //	fn debug_string(&self) -> String;

    //	fn merge_from(&mut self, other: &Self);

    // fn unknown_fields() -> &[UnknownField];
}

/// A pointer to a Message. Used in message fields to support storing possibly
/// recursive type usages.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct MessagePtr<T> {
    value: Box<T>,
}

impl<T> MessagePtr<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: Box::new(value),
        }
    }
}

impl<T> std::ops::Deref for MessagePtr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for MessagePtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> std::convert::AsRef<T> for MessagePtr<T> {
    fn as_ref(&self) -> &T {
        self.value.as_ref()
    }
}

impl<T> std::convert::AsMut<T> for MessagePtr<T> {
    fn as_mut(&mut self) -> &mut T {
        self.value.as_mut()
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
