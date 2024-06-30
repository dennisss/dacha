use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use common::any::AsAny;
use common::const_default::{ConstDefault, StaticDefault};
use core::any::Any;
use core::convert::Infallible;
use core::default::Default;
use core::ops::{Deref, DerefMut};

use common::bytes::BytesMut;
use common::collections::FixedString;
use common::fixed::vec::FixedVec;
use common::list::List;

use crate::extension::ExtensionSet;
use crate::message::Enum;
use crate::types::FieldNumber;
use crate::unknown::UnknownFieldSet;
use crate::Message;

// TODO: Rename to align with the protobuf types.
pub enum Reflection<'a> {
    F32(&'a f32),
    F64(&'a f64),
    I32(&'a i32),
    I64(&'a i64),
    U32(&'a u32),
    U64(&'a u64),
    Bool(&'a bool),
    String(&'a str),
    Bytes(&'a [u8]),
    Repeated(&'a dyn RepeatedFieldReflection),
    Message(&'a dyn MessageReflection),
    Enum(&'a dyn Enum),
    // Map(&'a dyn MapFieldReflection),
    Set(&'a dyn SetFieldReflection),
}

pub enum ReflectionMut<'a> {
    F32(&'a mut f32),
    F64(&'a mut f64),
    I32(&'a mut i32),
    I64(&'a mut i64),
    U32(&'a mut u32),
    U64(&'a mut u64),
    Bool(&'a mut bool),
    String(&'a mut String),
    Bytes(&'a mut dyn List<u8, Error = Infallible>),
    Repeated(&'a mut dyn RepeatedFieldReflection),
    Message(&'a mut dyn MessageReflection),
    Enum(&'a mut dyn Enum),
    // Map(&'a mut dyn MapFieldReflection),
    Set(&'a mut dyn SetFieldReflection), /* NOTE: reflect_mut() on an option will simply assign
                                          * a new default value.
                                          * TODO: Support controlling presence with reflection?
                                          * Option(Option<&'a mut dyn Reflect>) */
}

#[derive(Clone)]
pub struct FieldDescriptorShort {
    pub number: FieldNumber,
    pub name: StringPtr,
}

impl FieldDescriptorShort {
    pub fn new(name: String, number: FieldNumber) -> Self {
        Self {
            name: StringPtr::Dynamic(name),
            number,
        }
    }
}

#[derive(Clone)]
pub enum StringPtr {
    Static(&'static str),
    Dynamic(String),
}

impl PartialEq for StringPtr {
    fn eq(&self, other: &Self) -> bool {
        let a: &str = &*self;
        let b: &str = &*other;
        a == b
    }
}

impl std::ops::Deref for StringPtr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            StringPtr::Static(s) => *s,
            StringPtr::Dynamic(s) => s.as_ref(),
        }
    }
}

/// Enables dynamic retrieval of fields in a message.
///
/// - Should be implemented by all Messages.
/// - Note that unlike the mainline C++ implementation, '.*field.*' methods in
///   this trait do NOT include unknown fields or extensions.
///   - The user should use the unknown_fields() and extensions() to get these.
pub trait MessageReflection: Message + AsAny + MessageEquals {
    // A non-mutable version would be required for the regular

    // Should also have a fields() which iterates over fields?

    // Some fields may also have an empty name to indicate that they are unknown

    // List of all fields declared in the message definition.
    //
    // This includes fields that may not be present in the current message or are
    // set to the default value.
    fn fields(&self) -> &[FieldDescriptorShort];

    /// Checks if
    ///
    /// NOTE: This will also return false for unknown fields.
    fn has_field_with_number(&self, num: FieldNumber) -> bool;

    fn clear_field_with_number(&mut self, num: FieldNumber);

    /// Gets the value of a field given its field number.
    ///
    /// - If the field is not present, returns a default value.
    /// - Returns None only if an unknown field number was specified.
    fn field_by_number(&self, num: FieldNumber) -> Option<Reflection>;

    fn field_by_number_mut(&mut self, num: FieldNumber) -> Option<ReflectionMut>;

    fn field_number_by_name(&self, name: &str) -> Option<FieldNumber>;

    fn unknown_fields(&self) -> Option<&UnknownFieldSet>;

    fn extensions(&self) -> Option<&ExtensionSet>;

    fn extensions_mut(&mut self) -> Option<&mut ExtensionSet>;

    // TODO: Find a better name for this.
    #[cfg(feature = "alloc")]
    fn box_clone2(&self) -> Box<dyn MessageReflection>;
}

pub trait MessageEquals {
    fn message_equals(&self, other: &dyn MessageReflection) -> bool;
}

impl<M: Message + PartialEq<M> + 'static> MessageEquals for M {
    fn message_equals(&self, other: &dyn MessageReflection) -> bool {
        let any = other.as_any();
        if let Some(rhs) = any.downcast_ref::<M>() {
            self == rhs
        } else {
            false
        }
    }
}

/// Trivially downcasts a type to its Reflection/ReflectionMut representation.
///
/// INTERNAL TYPE: Mainly to be used in generated code.
pub trait Reflect {
    fn reflect(&self) -> Reflection;

    // TODO: Split into a separate ReflectMut trait.
    fn reflect_mut(&mut self) -> ReflectionMut;
}

/// Static methods implemented on types used inside of Message structs.
///
/// INTERNAL TYPE: Mainly to be used in generated code.
pub trait ReflectStatic {
    type Type: ?Sized + Reflect;

    /// Gets a reference to the default value of this field type.
    /// Note that non-default constructable types will return a different type
    /// than Self.
    fn reflect_static_default() -> &'static Self::Type;
}

macro_rules! define_reflect {
    ($name:ident, $t:ident, $y:ident, $default:expr) => {
        impl Reflect for $t {
            fn reflect(&self) -> Reflection {
                Reflection::$name(self)
            }
            fn reflect_mut(&mut self) -> ReflectionMut {
                ReflectionMut::$name(self)
            }
        }

        impl ReflectStatic for $t {
            type Type = $y;

            fn reflect_static_default() -> &'static Self::Type {
                &$default
            }
        }
    };
}

define_reflect!(F32, f32, f32, 0.0);
define_reflect!(F64, f64, f64, 0.0);
define_reflect!(I32, i32, i32, 0);
define_reflect!(I64, i64, i64, 0);
define_reflect!(U32, u32, u32, 0);
define_reflect!(U64, u64, u64, 0);
define_reflect!(Bool, bool, bool, false);
define_reflect!(String, String, str, "");

impl Reflect for str {
    fn reflect(&self) -> Reflection {
        Reflection::String(self)
    }

    // TODO: Split up Reflect and ReflectMut so that this isn't needed.
    fn reflect_mut(&mut self) -> ReflectionMut {
        panic!()
    }
}

impl Reflect for [u8] {
    fn reflect(&self) -> Reflection {
        Reflection::Bytes(self)
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        panic!()
    }
}

impl Reflect for crate::bytes::BytesField {
    fn reflect(&self) -> Reflection {
        Reflection::Bytes(self.0.as_ref())
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Bytes(&mut self.0)
    }
}

impl ReflectStatic for crate::bytes::BytesField {
    type Type = [u8];

    fn reflect_static_default() -> &'static Self::Type {
        &[]
    }
}

impl<T: MessageReflection> Reflect for T {
    fn reflect(&self) -> Reflection {
        Reflection::Message(self)
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Message(self)
    }
}

impl<T: Reflect> Reflect for crate::MessagePtr<T> {
    fn reflect(&self) -> Reflection {
        self.deref().reflect()
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        self.deref_mut().reflect_mut()
    }
}

impl<T: Reflect + StaticDefault> ReflectStatic for crate::message::MessagePtr<T> {
    type Type = T;

    fn reflect_static_default() -> &'static Self::Type {
        T::static_default()
    }
}

impl<T: Reflect + Default> Reflect for Vec<T> {
    fn reflect(&self) -> Reflection {
        Reflection::Repeated(self)
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Repeated(self)
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> Reflect for FixedString<A> {
    fn reflect(&self) -> Reflection {
        Reflection::String(self.as_ref())
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        todo!()
    }
}

// Used for 'bytes' types with the fixed_length option specified.
impl<const LEN: usize> Reflect for FixedVec<u8, LEN> {
    fn reflect(&self) -> Reflection {
        Reflection::Bytes(self.as_ref())
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Bytes(self)
    }
}

impl<T: Reflect + Default, const LEN: usize> Reflect for FixedVec<T, LEN> {
    fn reflect(&self) -> Reflection {
        Reflection::Repeated(self)
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Repeated(self)
    }
}

/// This trait is implemented on types that store the value of a protobuf
/// message field.
///
/// NOTE: It is only correct for this to be used in the internal generated
/// message code directly on the raw struct fields.
pub trait MessageFieldReflection {
    fn reflect_has_field(&self) -> bool;

    fn reflect_field(&self) -> Reflection;

    fn reflect_field_mut(&mut self) -> ReflectionMut;

    fn reflect_clear_field(&mut self);
}

// Option<T>
// - This is used for all singular fields in proto2.
// - Only explicitly optional and message types use this in proto3.
//
// In all cases, field presence is straight forward since it is explicitly
// encoded in the Option.
impl<T: 'static + Reflect + Default + ReflectStatic> MessageFieldReflection for Option<T> {
    fn reflect_has_field(&self) -> bool {
        self.is_some()
    }

    fn reflect_field(&self) -> Reflection {
        match self {
            Some(v) => v.reflect(),
            None => T::reflect_static_default().reflect(),
        }
    }

    fn reflect_field_mut(&mut self) -> ReflectionMut {
        let v = match self {
            Some(v) => v,
            // TODO: If an explicit default value is available, we should use that instead.
            None => self.insert(T::default()),
        };

        v.reflect_mut()
    }

    fn reflect_clear_field(&mut self) {
        *self = None;
    }
}

// Regular non-Option values.
// - In proto2/proto3, this is used for all repeated fields.
// - In proto3, this is used for all primitive non-explicitly optional fields.
impl<T: Reflect + Default + PartialEq> MessageFieldReflection for T {
    fn reflect_has_field(&self) -> bool {
        *self != T::default()
    }

    fn reflect_field(&self) -> Reflection {
        self.reflect()
    }

    fn reflect_field_mut(&mut self) -> ReflectionMut {
        self.reflect_mut()
    }

    // TODO: For repeated fields, it is more efficient to preserve the memory buffer
    // by calling .clear() on the Vec.
    fn reflect_clear_field(&mut self) {
        *self = T::default();
    }
}

pub trait RepeatedFieldReflection {
    fn reflect_len(&self) -> usize;
    fn reflect_get(&self, index: usize) -> Option<Reflection>;
    fn reflect_get_mut(&mut self, index: usize) -> Option<ReflectionMut>;
    fn reflect_add(&mut self) -> ReflectionMut;
}

impl<T: Reflect + Default> RepeatedFieldReflection for Vec<T> {
    fn reflect_len(&self) -> usize {
        Vec::len(self)
    }
    fn reflect_get(&self, index: usize) -> Option<Reflection> {
        // TODO: A repeated field should never contain an element that returns None?
        self.deref().get(index).map(|v: &T| v.reflect())
    }
    fn reflect_get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
        self.deref_mut()
            .get_mut(index)
            .map(|v: &mut T| v.reflect_mut())
    }
    fn reflect_add(&mut self) -> ReflectionMut {
        Vec::push(self, T::default());
        let idx = self.len() - 1;
        self[idx].reflect_mut()
    }
}

impl<T: Reflect + Default, const LEN: usize> RepeatedFieldReflection for FixedVec<T, LEN> {
    fn reflect_len(&self) -> usize {
        let s: &[T] = self.as_ref();
        s.len()
    }

    fn reflect_get(&self, index: usize) -> Option<Reflection> {
        self.deref().get(index).map(|v: &T| v.reflect())
    }
    fn reflect_get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
        self.deref_mut()
            .get_mut(index)
            .map(|v: &mut T| v.reflect_mut())
    }
    fn reflect_add(&mut self) -> ReflectionMut {
        FixedVec::push(self, T::default());
        let idx = self.len() - 1;
        self[idx].reflect_mut()
    }
}

/*
pub trait SetFieldReflection {
    fn len(&self) -> usize;

    fn entry<'a>(&'a self) -> Box<dyn SetFieldEntryReflection + 'a>;

    fn entry_mut<'a>(&'a mut self) -> Box<dyn SetFieldEntryReflectionMut + 'a>;

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = Reflection<'a>> + 'a>;
}
*/

pub trait SetFieldReflection {
    fn len(&self) -> usize;

    fn entry<'a>(&'a self) -> Box<dyn SetFieldEntryReflection + 'a>;

    fn entry_mut<'a>(&'a mut self) -> Box<dyn SetFieldEntryReflectionMut + 'a>;

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = Reflection<'a>> + 'a>;
}

pub trait SetFieldEntryReflection {
    fn value(&mut self) -> ReflectionMut;

    fn contains(&self) -> bool;
}

pub trait SetFieldEntryReflectionMut: SetFieldEntryReflection {
    fn insert(&mut self) -> bool;

    fn remove(&mut self) -> bool;
}
