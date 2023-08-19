use std::collections::HashMap;
use std::fmt::Write;
use std::marker::PhantomData;

use common::errors::*;
use reflection::SerializeTo;

use crate::value::Value;

#[derive(Default)]
pub struct StringifyOptions {
    pub indent: Option<String>,
    pub space_after_colon: bool,
}

pub struct Stringifier {
    output: String,
    options: StringifyOptions,
    current_indent_level: usize,
}

impl Stringifier {
    pub fn new(options: StringifyOptions) -> Self {
        Self {
            output: String::new(),
            options,
            current_indent_level: 0,
        }
    }

    /// NOTE: This should NOT be called twice.
    pub fn root_value<'a>(&'a mut self) -> ValueStringifier<'a> {
        // TODO: Verify that this is only ever called once.

        ValueStringifier { outer: self }
    }

    pub fn finish(self) -> String {
        self.output
    }

    pub fn run(value: &Value, options: StringifyOptions) -> String {
        let mut inst = Self::new(options);
        inst.add_value(value);
        inst.finish()
    }

    fn add_value(&mut self, value: &Value) {
        match value {
            Value::Null => {
                self.add_null();
            }
            Value::Bool(v) => {
                self.add_bool(*v);
            }
            Value::Number(v) => {
                self.add_number(*v);
            }
            Value::String(s) => {
                self.add_string(s.as_str());
            }
            Value::Object(obj) => {
                self.add_object(obj);
            }
            Value::Array(values) => {
                self.add_array(values);
            }
        }
    }

    fn add_indent(&mut self) {
        if let Some(i) = self.options.indent.as_ref() {
            self.output.push('\n');
            for _ in 0..self.current_indent_level {
                self.output.push_str(i.as_str());
            }
        }
    }

    fn add_null(&mut self) {
        self.output.push_str("null");
    }

    fn add_bool(&mut self, v: bool) {
        self.output.push_str(if v { "true" } else { "false" });
    }

    fn add_number(&mut self, v: f64) {
        write!(self.output, "{}", v).unwrap();
    }

    fn add_string(&mut self, s: &str) {
        self.output.reserve(s.len());
        self.output.push('"');

        for c in s.chars() {
            if (c as u32) >= 0x20 && c != '"' && c != '\\' {
                self.output.push(c);
            } else {
                match c {
                    '"' | '\\' | '/' => {
                        self.output.push('\\');
                        self.output.push(c);
                    }
                    '\x08' => {
                        self.output.push_str("\\b");
                    }
                    '\x0C' => {
                        self.output.push_str("\\f");
                    }
                    '\n' => {
                        self.output.push_str("\\n");
                    }
                    '\r' => {
                        self.output.push_str("\\r");
                    }
                    '\t' => {
                        self.output.push_str("\\t");
                    }
                    _ => {
                        write!(self.output, "\\u00{:02X}", ((c as u32) as u8)).unwrap();
                    }
                }
            }
        }

        self.output.push('"');
    }

    fn add_object_start(&mut self) {
        self.output.push('{');
        self.current_indent_level += 1;
    }

    fn add_object_field_key(&mut self, key: &str, first: bool) {
        if !first {
            self.output.push(',')
        }

        self.add_indent();
        self.add_string(key);
        self.output.push(':');
        if self.options.space_after_colon {
            self.output.push(' ');
        }
    }

    fn add_object_end(&mut self, was_empty: bool) {
        self.current_indent_level -= 1;
        if !was_empty {
            self.add_indent();
        }

        self.output.push('}');
    }

    fn add_object(&mut self, obj: &HashMap<String, Value>) {
        self.add_object_start();

        let mut first = true;
        for (key, value) in obj.iter() {
            self.add_object_field_key(key.as_str(), first);
            self.add_value(value);
            first = false;
        }

        self.add_object_end(obj.is_empty());
    }

    fn add_array_start(&mut self) {
        self.output.push('[');
        self.current_indent_level += 1;
    }

    fn add_array_before_element(&mut self, first: bool) {
        if !first {
            self.output.push(',')
        }

        self.add_indent();
    }

    fn add_array_end(&mut self, was_empty: bool) {
        self.current_indent_level -= 1;
        if !was_empty {
            self.add_indent();
        }

        self.output.push(']');
    }

    fn add_array(&mut self, values: &[Value]) {
        self.add_array_start();

        let mut first = true;
        for value in values {
            self.add_array_before_element(first);
            self.add_value(value);
            first = false;
        }

        self.add_array_end(values.is_empty());
    }
}

impl<'a> reflection::ValueSerializer for &'a mut Stringifier {
    type ObjectSerializerType = ObjectStringifier<'a>;
    type ListSerializerType = ArrayStringifier<'a>;

    fn serialize_primitive(self, value: reflection::PrimitiveValue) -> common::errors::Result<()> {
        self.root_value().serialize_primitive(value)
    }

    fn serialize_object(self) -> ObjectStringifier<'a> {
        self.root_value().serialize_object()
    }

    fn serialize_list(self) -> ArrayStringifier<'a> {
        self.root_value().serialize_list()
    }
}

/// Interface for serializing a single instance of a value.
pub struct ValueStringifier<'a> {
    outer: &'a mut Stringifier,
}

impl<'a> ValueStringifier<'a> {
    pub fn bool(self, value: bool) {
        self.outer.add_bool(value);
    }

    pub fn number(self, value: f64) {
        self.outer.add_number(value);
    }

    pub fn string(self, value: &str) {
        self.outer.add_string(value);
    }

    pub fn object(self) -> ObjectStringifier<'a> {
        self.outer.add_object_start();
        ObjectStringifier {
            outer: self.outer,
            first: true,
        }
    }

    pub fn array(self) -> ArrayStringifier<'a> {
        self.outer.add_array_start();
        ArrayStringifier {
            outer: self.outer,
            first: true,
        }
    }
}

impl<'a> reflection::ValueSerializer for ValueStringifier<'a> {
    type ObjectSerializerType = ObjectStringifier<'a>;
    type ListSerializerType = ArrayStringifier<'a>;

    fn serialize_primitive(self, value: reflection::PrimitiveValue) -> Result<()> {
        match value {
            reflection::PrimitiveValue::Null => self.outer.add_null(),
            reflection::PrimitiveValue::Bool(v) => self.bool(v),
            reflection::PrimitiveValue::I8(v) => self.number(v as f64),
            reflection::PrimitiveValue::U8(v) => self.number(v as f64),
            reflection::PrimitiveValue::I16(v) => self.number(v as f64),
            reflection::PrimitiveValue::U16(v) => self.number(v as f64),
            reflection::PrimitiveValue::I32(v) => self.number(v as f64),
            reflection::PrimitiveValue::U32(v) => self.number(v as f64),
            reflection::PrimitiveValue::I64(v) => self.number(v as f64),
            reflection::PrimitiveValue::U64(v) => self.number(v as f64),
            reflection::PrimitiveValue::ISize(v) => self.number(v as f64),
            reflection::PrimitiveValue::USize(v) => self.number(v as f64),
            reflection::PrimitiveValue::F32(v) => self.number(v as f64),
            reflection::PrimitiveValue::F64(v) => self.number(v as f64),
            reflection::PrimitiveValue::Str(v) => self.string(v),
            reflection::PrimitiveValue::String(v) => self.string(&v),
        }

        Ok(())
    }

    fn serialize_object(self) -> Self::ObjectSerializerType {
        self.object()
    }

    fn serialize_list(self) -> Self::ListSerializerType {
        self.array()
    }
}

pub struct ObjectStringifier<'a> {
    outer: &'a mut Stringifier,
    first: bool,
}

impl<'a> ObjectStringifier<'a> {
    pub fn key<'b>(&'b mut self, key: &str) -> ValueStringifier<'b> {
        self.outer.add_object_field_key(key, self.first);
        self.first = false;

        ValueStringifier { outer: self.outer }
    }
}

impl<'a> Drop for ObjectStringifier<'a> {
    fn drop(&mut self) {
        self.outer.add_object_end(self.first);
    }
}

impl<'a> reflection::ObjectSerializer for ObjectStringifier<'a> {
    fn serialize_field<Value: reflection::SerializeTo>(
        &mut self,
        name: &str,
        value: &Value,
    ) -> Result<()> {
        value.serialize_to(self.key(name))
    }
}

pub struct ArrayStringifier<'a> {
    outer: &'a mut Stringifier,
    first: bool,
}

impl<'a> ArrayStringifier<'a> {
    pub fn element<'b>(&'b mut self) -> ValueStringifier<'b> {
        self.outer.add_array_before_element(self.first);
        self.first = false;

        ValueStringifier { outer: self.outer }
    }
}

impl<'a> Drop for ArrayStringifier<'a> {
    fn drop(&mut self) {
        self.outer.add_array_end(self.first);
    }
}

impl<'a> reflection::ListSerializer for ArrayStringifier<'a> {
    fn serialize_element<Value: reflection::SerializeTo>(&mut self, value: &Value) -> Result<()> {
        value.serialize_to(self.element())
    }
}
