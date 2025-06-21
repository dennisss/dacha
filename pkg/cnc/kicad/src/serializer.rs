use parsing::*;
use common::errors::*;

pub struct SExprSerializer {
    output: String
}

impl SExprSerializer {
    pub fn new() -> Self {
        Self { output: String::new() }
    }

    pub fn root<'a>(&'a mut self, token: &str) -> Result<SExprListSerializer<'a>> {
        self.output.push('(');
        let mut s = SExprListSerializer { outer: self, empty: true, indent: true };
        s.string(token, false)?;
        Ok(s)
    }

    pub fn finish(self) -> String {
        self.output
    }
}


pub struct SExprListSerializer<'a> {
    outer: &'a mut SExprSerializer,
    empty: bool,
    indent: bool,
}

impl<'a> Drop for SExprListSerializer<'a> {
    fn drop(&mut self) {
        self.outer.output.push(')');
    }
}

impl<'a> SExprListSerializer<'a> {

    fn before_element(&mut self) {
        if !self.empty {
            if self.indent {
                self.outer.output.push_str("\n  ");
            } else {
                self.outer.output.push(' ');
            }
        }
        self.empty = false;
    }

    pub fn string(&mut self, v: &str, quoted: bool) -> Result<()> {
        self.before_element();
        
        let out = &mut self.outer.output;

        if quoted {
            out.push('"');

            for c in v.chars() {
                if c == '\\' {
                    out.push_str("\\\\");
                } else if c == '\n' {
                    out.push_str("\\n");
                } else if c == '\r' {
                    out.push_str("\\r");
                } else if c == '"' {
                    out.push_str("\\\"");
                } else {
                    out.push(c);
                }
            }

            out.push('"');
        } else {
            for c in v.chars() {
                if c.is_ascii_whitespace() || c == '"' {
                    return Err(err_msg("String must be quoted"));
                }
            }

            out.push_str(v);
        }

        Ok(())
    }

    pub fn list<'b>(&'b mut self) -> SExprListSerializer<'b> {
        self.before_element();

        self.outer.output.push('(');
        SExprListSerializer { outer: self.outer, empty: true, indent: false, }
    }
}


impl<'a> reflection::ValueSerializer for SExprListSerializer<'a> {
    type ObjectSerializerType = ObjectStringifier<'a>;
    type ListSerializerType = ArrayStringifier<'static, 'static>;

    fn serialize_primitive(mut self, value: reflection::PrimitiveValue) -> Result<()> {
        // This should never be called since we can only serialize these as part of fields.
        todo!()
    }

    fn serialize_object(self) -> Self::ObjectSerializerType {
        // This is only called on the root() element.
        ObjectStringifier { list: self }
    }

    fn serialize_list(self) -> Self::ListSerializerType {
        // This should never be called since we can only serialize these as part of fields.
        todo!()
    }
}

pub struct ObjectStringifier<'a> {
    list: SExprListSerializer<'a>,
}

impl<'a> reflection::ObjectSerializer for ObjectStringifier<'a> {
    fn serialize_field<Value: reflection::SerializeTo>(
        &mut self,
        name: &str,
        value: &Value,
    ) -> Result<()> {
        let field = FieldSerializer {
            token: name.to_string(),
            list: &mut self.list
        };
        value.serialize_to(field);
        
        Ok(())
    }
}

pub struct FieldSerializer<'a, 'b> {
    token: String,
    list: &'b mut SExprListSerializer<'a>,
}

impl<'a, 'b> reflection::ValueSerializer for FieldSerializer<'a, 'b> {
    type ObjectSerializerType = ObjectStringifier<'b>;
    type ListSerializerType = ArrayStringifier<'a, 'b>;

    fn serialize_primitive(mut self, value: reflection::PrimitiveValue) -> Result<()> {
        let mut list = self.list.list();
        list.string(&self.token, false)?;

        match value {
            reflection::PrimitiveValue::Null => todo!(),
            reflection::PrimitiveValue::Bool(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::I8(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::U8(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::I16(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::U16(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::I32(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::U32(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::I64(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::U64(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::ISize(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::USize(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::F32(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::F64(v) => list.string(&v.to_string(), false),
            reflection::PrimitiveValue::Str(v) => list.string(v, true),
            reflection::PrimitiveValue::String(v) => list.string(&v, true),
        }?;

        Ok(())
    }

    fn serialize_object(self) -> Self::ObjectSerializerType {
        let mut list = self.list.list();
        list.string(&self.token, false).unwrap();

        ObjectStringifier { list }
    }

    fn serialize_list(self) -> Self::ListSerializerType {
        ArrayStringifier { token: self.token, list: self.list }
    }
}


pub struct ArrayStringifier<'a, 'b> {
    token: String,
    list: &'b mut SExprListSerializer<'a>,
}

impl<'a, 'b> reflection::ListSerializer for ArrayStringifier<'a, 'b> {
    fn serialize_element<Value: reflection::SerializeTo>(&mut self, value: &Value) -> Result<()> {
        let field = FieldSerializer {
            token: self.token.clone(),
            list: self.list
        };
        value.serialize_to(field)
    }
}
