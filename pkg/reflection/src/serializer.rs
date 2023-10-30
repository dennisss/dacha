use std::collections::HashMap;

use common::errors::*;

use crate::parser::PrimitiveValue;

pub trait SerializeTo {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()>;

    fn serialize_as_empty_value(&self) -> bool {
        false
    }

    // TODO: This is only really supported by primitive types and may be mistakenly
    // used by other types so consider making this a separate trait.
    fn serialize_sparse_as_empty_value(&self) -> bool {
        false
    }
}

pub trait ValueSerializer {
    type ObjectSerializerType: ObjectSerializer;
    type ListSerializerType: ListSerializer;

    fn serialize_primitive(self, value: PrimitiveValue) -> Result<()>;

    fn serialize_object(self) -> Self::ObjectSerializerType;

    fn serialize_list(self) -> Self::ListSerializerType;
}

pub trait ObjectSerializer {
    fn serialize_field<Value: SerializeTo>(&mut self, name: &str, value: &Value) -> Result<()>;
}

pub trait ListSerializer {
    fn serialize_element<Value: SerializeTo>(&mut self, value: &Value) -> Result<()>;
}

macro_rules! impl_primitive_serialize_to {
    ($t:ty, $case:ident) => {
        impl SerializeTo for $t {
            fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
                out.serialize_primitive(PrimitiveValue::$case(*self))
            }

            fn serialize_sparse_as_empty_value(&self) -> bool {
                *self == Self::default()
            }
        }
    };
}

impl_primitive_serialize_to!(bool, Bool);
impl_primitive_serialize_to!(i8, I8);
impl_primitive_serialize_to!(u8, U8);
impl_primitive_serialize_to!(i16, I16);
impl_primitive_serialize_to!(u16, U16);
impl_primitive_serialize_to!(i32, I32);
impl_primitive_serialize_to!(u32, U32);
impl_primitive_serialize_to!(i64, I64);
impl_primitive_serialize_to!(u64, U64);
impl_primitive_serialize_to!(f32, F32);
impl_primitive_serialize_to!(f64, F64);
impl_primitive_serialize_to!(isize, ISize);
impl_primitive_serialize_to!(usize, USize);

impl SerializeTo for () {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        Ok(())
    }

    fn serialize_as_empty_value(&self) -> bool {
        true
    }

    fn serialize_sparse_as_empty_value(&self) -> bool {
        true
    }
}

impl<T: SerializeTo> SerializeTo for Option<T> {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        match self {
            Some(v) => v.serialize_to(out),
            None => Err(err_msg("Can't serialize None value")),
        }
    }

    fn serialize_as_empty_value(&self) -> bool {
        self.is_none()
    }

    fn serialize_sparse_as_empty_value(&self) -> bool {
        self.is_none()
    }
}

impl SerializeTo for String {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        out.serialize_primitive(PrimitiveValue::String(self.to_string()))
    }

    fn serialize_sparse_as_empty_value(&self) -> bool {
        self.is_empty()
    }
}

impl<T: SerializeTo> SerializeTo for Vec<T> {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        let mut list = out.serialize_list();
        for value in self {
            list.serialize_element(value)?;
        }
        Ok(())
    }

    fn serialize_sparse_as_empty_value(&self) -> bool {
        self.is_empty()
    }
}

impl<T: SerializeTo> SerializeTo for Box<T> {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        T::serialize_to(self.as_ref(), out)
    }
}

impl<T: SerializeTo> SerializeTo for HashMap<String, T> {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        let mut obj = out.serialize_object();
        for (key, value) in self {
            obj.serialize_field(key.as_str(), value)?;
        }

        Ok(())
    }

    fn serialize_sparse_as_empty_value(&self) -> bool {
        self.is_empty()
    }
}

struct Hello {
    name: String,
}

impl SerializeTo for Hello {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        let mut obj = out.serialize_object();
        obj.serialize_field("hi", &self.name)?;
        obj.serialize_field("hello", &self.name)?;
        Ok(())
    }
}
