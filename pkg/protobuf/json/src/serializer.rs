use std::sync::Arc;
use std::{borrow::BorrowMut, collections::HashMap};

use common::errors::*;
use protobuf::{message_factory::MessageFactory, reflection::*, FieldNumber};

// TODO: Implement usage of this.
#[derive(Default, Clone)]
pub struct SerializerOptions {
    pub emit_enum_integers: bool,
    pub emit_default_values_fields: bool,
    pub use_original_field_names: bool,

    /// Factory to use for looking up Any proto types.
    /// If not provided, serialization of Any protos will fail.
    pub message_factory: Option<Arc<dyn MessageFactory>>,
}

pub trait MessageJsonSerialize {
    fn serialize_json(&self, options: &SerializerOptions) -> Result<String>;
}

impl<M: MessageReflection> MessageJsonSerialize for M {
    fn serialize_json(&self, options: &SerializerOptions) -> Result<String> {
        // TODO: Implement this using incremental json string building to avoid
        // expensive temporaries.

        let mut stringifier = json::Stringifier::new(json::StringifyOptions::default());

        let obj = stringifier.root_value().object();
        message_to_json_value(self, options, obj, false)?;

        Ok(stringifier.finish())
    }
}

fn message_to_json_value(
    message: &dyn MessageReflection,
    options: &SerializerOptions,
    mut output: json::ObjectStringifier,
    in_any_proto: bool,
) -> Result<()> {
    /*
    TODO: Need special cases for all the builtin message types mentioned in https://protobuf.dev/programming-guides/proto3/#json
    */

    if message.type_url() == "type.googleapis.com/google.protobuf.Any" {
        return serialize_any(message, options, output, false);
    }

    for field_desc in message.fields() {
        if !message.has_field_with_number(field_desc.number) {
            continue;
        }

        let field = message.field_by_number(field_desc.number).unwrap();

        let value = output.key(&*field_desc.name);
        reflection_to_json_value(field, options, value)?;
    }

    Ok(())
}

fn serialize_any(
    message: &dyn MessageReflection,
    options: &SerializerOptions,
    mut output: json::ObjectStringifier,
    in_any_proto: bool,
) -> Result<()> {
    const ANY_TYPE_URL_FIELD_NUM: FieldNumber = 1; // Any::TYPE_URL_FIELD_NUM
    const ANY_VALUE_FIELD_NUM: FieldNumber = 2; // Any::VALUE_FIELD_NUM

    // TODO: Instead messages with special serializations like
    // Any/Duration/Timestamp should use the 'value' json field.
    if in_any_proto {
        return Err(err_msg("Can not serialize an Any proto in an Any proto"));
    }

    let message_factory = match options.message_factory.clone() {
        Some(v) => v,
        None => {
            return Err(err_msg(
                "Can not json serialize Any proto without a message factory.",
            ))
        }
    };

    let type_url = match message.field_by_number(ANY_TYPE_URL_FIELD_NUM) {
        Some(Reflection::String(s)) => s,
        _ => return Err(err_msg("Unable to get type url in Any proto")),
    };

    let value = match message.field_by_number(ANY_VALUE_FIELD_NUM) {
        Some(Reflection::Bytes(v)) => v,
        _ => return Err(err_msg("Unable to get value in Any proto")),
    };

    let mut inner_message = message_factory
        .new_message(type_url)
        .ok_or_else(|| format_err!("No message factory for type: {}", type_url))?;
    inner_message.parse_merge(value)?;

    output.key("@type").string(type_url);

    message_to_json_value(inner_message.as_ref(), options, output, true)?;

    Ok(())
}

fn reflection_to_json_value(
    r: Reflection,
    options: &SerializerOptions,
    output: json::ValueStringifier,
) -> Result<()> {
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
            for i in 0..v.reflect_len() {
                let el = arr.element();
                reflection_to_json_value(v.reflect_get(i).unwrap(), options, el)?;
            }
        }
        Reflection::Message(v) => {
            let obj = output.object();
            message_to_json_value(v, options, obj, false)?;
        }
        Reflection::Enum(v) => {
            output.string(v.name());
        }
        Reflection::Set(_) => todo!(),
    }

    Ok(())
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
