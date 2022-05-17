use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::convert::Infallible;
use core::default::Default;
use core::ops::{Deref, DerefMut};

use common::bytes::BytesMut;
use common::collections::FixedString;
use common::fixed::vec::FixedVec;
use common::list::List;

use crate::message::Enum;
use crate::types::FieldNumber;

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
    Set(&'a mut dyn SetFieldReflection), /* NOTE: reflect_mut() on an option will simply assign
                                          * a new default value.
                                          * TODO: Support controlling presence with reflection?
                                          * Option(Option<&'a mut dyn Reflect>) */
}

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

pub enum StringPtr {
    Static(&'static str),
    Dynamic(String),
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

/// NOTE: Should be implemented by all Messages.
pub trait MessageReflection {
    // A non-mutable version would be required for the regular

    // Should also have a fields() which iterates over fields?

    // Some fields may also have an empty name to indicate that they are unknown

    // List of all fields declared in the message definition.
    //
    // This includes fields that may not be present in the current message or are
    // set to the default value.
    fn fields(&self) -> &[FieldDescriptorShort];

    /// Returns None if the field is now defined in the descriptor or the field
    /// doesn't have a value (based on field presence rules).
    fn field_by_number(&self, num: FieldNumber) -> Option<Reflection>;

    fn field_by_number_mut(&mut self, num: FieldNumber) -> Option<ReflectionMut>;

    fn field_number_by_name(&self, name: &str) -> Option<FieldNumber>;
}

pub trait Reflect {
    fn reflect(&self) -> Reflection;
    fn reflect_mut(&mut self) -> ReflectionMut;
}

macro_rules! define_reflect {
    ($name:ident, $t:ident) => {
        impl Reflect for $t {
            fn reflect(&self) -> Reflection {
                Reflection::$name(self)
            }
            fn reflect_mut(&mut self) -> ReflectionMut {
                ReflectionMut::$name(self)
            }
        }
    };
}

define_reflect!(F32, f32);
define_reflect!(F64, f64);
define_reflect!(I32, i32);
define_reflect!(I64, i64);
define_reflect!(U32, u32);
define_reflect!(U64, u64);
define_reflect!(Bool, bool);
define_reflect!(String, String);

impl Reflect for crate::bytes::BytesField {
    fn reflect(&self) -> Reflection {
        Reflection::Bytes(self.0.as_ref())
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Bytes(&mut self.0)
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

pub trait SingularFieldReflectionProto2 {
    fn reflect_field_proto2(&self) -> Option<Reflection>;
    fn reflect_field_mut_proto2(&mut self) -> ReflectionMut;
}

impl<T: Reflect + Default> SingularFieldReflectionProto2 for Option<T> {
    fn reflect_field_proto2(&self) -> Option<Reflection> {
        self.as_ref().map(|v| v.reflect())
    }
    fn reflect_field_mut_proto2(&mut self) -> ReflectionMut {
        if !self.is_some() {
            // TODO: If an explicit default value is available, we should use that instead.
            *self = Some(T::default());
        }

        self.as_mut().unwrap().reflect_mut()
    }
}

impl<T: Reflect> SingularFieldReflectionProto2 for T {
    fn reflect_field_proto2(&self) -> Option<Reflection> {
        Some(self.reflect())
    }
    fn reflect_field_mut_proto2(&mut self) -> ReflectionMut {
        self.reflect_mut()
    }
}

pub trait SingularFieldReflectionProto3 {
    fn reflect_field_proto3(&self) -> Option<Reflection>;
    fn reflect_field_mut_proto3(&mut self) -> ReflectionMut;
}

// This should only apply to embedded messages in Proto3.
impl<T: Reflect + Default> SingularFieldReflectionProto3 for Option<T> {
    fn reflect_field_proto3(&self) -> Option<Reflection> {
        self.as_ref().map(|v| v.reflect())
    }
    fn reflect_field_mut_proto3(&mut self) -> ReflectionMut {
        if !self.is_some() {
            *self = Some(T::default());
        }

        self.as_mut().unwrap().reflect_mut()
    }
}

// TODO: Make sure that this doesn't accidentally get used for repeated fields.
impl<T: Reflect + Default + PartialEq> SingularFieldReflectionProto3 for T {
    fn reflect_field_proto3(&self) -> Option<Reflection> {
        if *self == T::default() {
            return None;
        }

        Some(self.reflect())
    }
    fn reflect_field_mut_proto3(&mut self) -> ReflectionMut {
        self.reflect_mut()
    }
}

pub trait RepeatedFieldReflection {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<Reflection>;
    fn get_mut(&mut self, index: usize) -> Option<ReflectionMut>;
    fn add(&mut self) -> ReflectionMut;
}

impl<T: Reflect + Default> RepeatedFieldReflection for Vec<T> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn get(&self, index: usize) -> Option<Reflection> {
        // TODO: A repeated field should never contain an element that returns None?
        self.deref().get(index).map(|v: &T| v.reflect())
    }
    fn get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
        self.deref_mut()
            .get_mut(index)
            .map(|v: &mut T| v.reflect_mut())
    }
    fn add(&mut self) -> ReflectionMut {
        Vec::push(self, T::default());
        let idx = self.len() - 1;
        self[idx].reflect_mut()
    }
}

impl<T: Reflect + Default, const LEN: usize> RepeatedFieldReflection for FixedVec<T, LEN> {
    fn len(&self) -> usize {
        let s: &[T] = self.as_ref();
        s.len()
    }

    fn get(&self, index: usize) -> Option<Reflection> {
        self.deref().get(index).map(|v: &T| v.reflect())
    }
    fn get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
        self.deref_mut()
            .get_mut(index)
            .map(|v: &mut T| v.reflect_mut())
    }
    fn add(&mut self) -> ReflectionMut {
        FixedVec::push(self, T::default());
        let idx = self.len() - 1;
        self[idx].reflect_mut()
    }
}

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
