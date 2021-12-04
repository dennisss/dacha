use common::errors::*;

use crate::parser::PrimitiveValue;

pub trait SerializeTo {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()>;
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

impl SerializeTo for String {
    fn serialize_to<Output: ValueSerializer>(&self, out: Output) -> Result<()> {
        out.serialize_primitive(PrimitiveValue::String(self.to_string()))
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
