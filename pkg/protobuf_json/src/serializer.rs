use std::{borrow::BorrowMut, collections::HashMap};

use protobuf::reflection::*;

// TODO: Implement usage of this.
#[derive(Default, Debug, Clone)]
pub struct SerializerOptions {
    pub emit_enum_integers: bool,
    pub emit_default_values_fields: bool,
    pub use_original_field_names: bool,
}

pub trait MessageJsonSerialize {
    fn serialize_json(&self) -> String;
}

impl<M: MessageReflection> MessageJsonSerialize for M {
    fn serialize_json(&self) -> String {
        // TODO: Implement this using incremental json string building to avoid
        // expensive temporaries.

        let mut stringifier = json::Stringifier::new(json::StringifyOptions::default());

        let obj = stringifier.root_value().object();
        message_to_json_value(self, obj);

        stringifier.finish()
    }
}

fn message_to_json_value(message: &dyn MessageReflection, mut output: json::ObjectStringifier) {
    for field_desc in message.fields() {
        let field = match message.field_by_number(field_desc.number) {
            Some(f) => f,
            None => continue,
        };

        let value = output.key(&*field_desc.name);
        reflection_to_json_value(field, value);
    }
}

fn reflection_to_json_value(r: Reflection, output: json::ValueStringifier) {
    match r {
        // TODO: Special cases for NaN and Infinity
        Reflection::F32(v) => f64_to_json_value(*v as f64, output),
        Reflection::F64(v) => f64_to_json_value(*v, output),
        Reflection::I32(v) => output.number(*v as f64),
        Reflection::U32(v) => output.number(*v as f64),
        Reflection::I64(v) => {
            output.string(&v.to_string());
        }
        Reflection::U64(v) => {
            output.string(&v.to_string());
        }
        Reflection::Bool(v) => {
            output.bool(*v);
        }
        Reflection::String(v) => {
            output.string(v);
        }
        Reflection::Bytes(v) => {
            // TODO: Perform simultaneous serialization and base64 encoding directly into
            // the json output buffer.
            let s = common::base64::encode_config(v, common::base64::URL_SAFE_NO_PAD);
            output.string(&s);
        }
        Reflection::Repeated(v) => {
            let mut arr = output.array();
            for i in 0..v.len() {
                let el = arr.element();
                reflection_to_json_value(v.get(i).unwrap(), el);
            }
        }
        Reflection::Message(v) => {
            let obj = output.object();
            message_to_json_value(v, obj);
        }
        Reflection::Enum(v) => {
            output.string(v.name());
        }
        Reflection::Set(_) => todo!(),
    }
}

fn f64_to_json_value(v: f64, output: json::ValueStringifier) {
    if v == f64::INFINITY {
        output.string("Infinity");
    } else if v == f64::NEG_INFINITY {
        output.string("-Infinity");
    } else if v == f64::NAN {
        output.string("NaN");
    } else {
        output.number(v);
    }
}
