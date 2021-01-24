use crate::spec::FieldNumber;
use crate::Enum;
use std::default::Default;
use std::ops::{Deref, DerefMut};

pub enum Reflection<'a> {
    F32(&'a f32),
    F64(&'a f64),
    I32(&'a i32),
    I64(&'a i64),
    U32(&'a u32),
    U64(&'a u64),
    Bool(&'a bool),
    String(&'a String),
    Bytes,
    Repeated(&'a dyn RepeatedFieldReflection),
    Message(&'a dyn MessageReflection),
    Enum(&'a dyn Enum),
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
    Bytes,
    Repeated(&'a mut dyn RepeatedFieldReflection),
    Message(&'a mut dyn MessageReflection),
    Enum(&'a mut dyn Enum),
}

// pub struct FieldReflection {
//     pub name: &'static str,
//     pub number: FieldNumber
// }

/// NOTE: Should be implemented by all Messages.
pub trait MessageReflection {
    // A non-mutable version would be required for the regular

    // Should also have a fields() which iterates over fields?

    // Some fields may also have an empty name to indicate that they are unknown
    //	fn fields(&self) -> &'static [(&FieldNumber)];

    fn field_by_number(&self, num: FieldNumber) -> Option<Reflection>;

    fn field_by_number_mut(&mut self, num: FieldNumber) -> Option<ReflectionMut>;

    fn field_number_by_name(&self, name: &str) -> Option<FieldNumber>;

    //	fn field_by_name_mut(&mut self, name: &str) -> Option<Reflection>;
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

impl<T: MessageReflection> Reflect for T {
    fn reflect(&self) -> Reflection {
        Reflection::Message(self)
    }
    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Message(self)
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
