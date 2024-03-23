use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use protobuf::message_factory::MessageFactory;
use protobuf::reflection::Reflect;
use protobuf::reflection::ReflectionMut;
use protobuf::EnumValue;
use protobuf::FieldNumber;
use protobuf::MessageReflection;

// TODO: Support field names that are translated to lowerCamelCase
// TODO: We should just sanitize in the protobuf compiler that all field names
// are unique after ignoring case and delimiters

// TODO: Support the 'json_name' option

macro_rules! integer_parser {
    ($r:ident, $value:ident, $t:ty) => {{
        *$r = match $value {
            json::Value::Number(v) => {
                let num = *v as $t;
                if (num as f64) != *v {
                    return Err(err_msg("Invalid integer"));
                }

                num
            }
            json::Value::String(v) => v.parse()?,
            _ => {
                return Err(err_msg("Unsupported json value for integer"));
            }
        }
    }};
}

// TODO: Start using these.
#[derive(Default)]
pub struct ParserOptions {
    pub ignore_unknown_fields: bool,

    /// Message factory to use for resolving Any protos which are inlined with
    /// '@type' object notation.
    pub message_factory: Option<Arc<dyn MessageFactory>>,
}

pub trait MessageJsonParser {
    fn parse_json(value: &str, options: &ParserOptions) -> Result<Self>
    where
        Self: Sized;
}

impl<M: Reflect + Default> MessageJsonParser for M {
    fn parse_json(value: &str, options: &ParserOptions) -> Result<Self> {
        let value = json::parse(value)?;
        let mut inst = M::default();
        apply_json_value_to_reflection(inst.reflect_mut(), &value, options)?;
        Ok(inst)
    }
}

fn apply_json_value_to_reflection(
    r: ReflectionMut,
    value: &json::Value,
    options: &ParserOptions,
) -> Result<()> {
    match r {
        ReflectionMut::F32(r) => {
            let double = get_f64(value)?;
            if double < (f32::MIN as f64) || double > (f32::MAX as f64) {
                return Err(err_msg("Value out of range for 32-bit float"));
            }

            *r = double as f32;
        }
        ReflectionMut::F64(r) => {
            *r = get_f64(value)?;
        }
        ReflectionMut::I32(r) => integer_parser!(r, value, i32),
        ReflectionMut::I64(r) => integer_parser!(r, value, i64),
        ReflectionMut::U32(r) => integer_parser!(r, value, u32),
        ReflectionMut::U64(r) => integer_parser!(r, value, u64),
        ReflectionMut::Bool(r) => match value {
            json::Value::Bool(v) => {
                *r = *v;
            }
            _ => {
                return Err(err_msg("Unsupported json value for bool"));
            }
        },
        ReflectionMut::String(r) => match value {
            json::Value::String(v) => {
                *r = v.clone();
            }
            _ => {
                return Err(err_msg("Unsupported json value for string"));
            }
        },
        ReflectionMut::Bytes(r) => {
            match value {
                json::Value::String(v) => {
                    // TODO: Verify that this can handle multiple different character sets.
                    let mut buf = vec![];
                    common::base64::decode_config_buf(
                        v.as_str(),
                        common::base64::URL_SAFE_NO_PAD,
                        &mut buf,
                    )?;
                    r.extend_from_slice(buf.as_ref());
                }
                _ => {
                    return Err(err_msg("Unsupported json value for bytes"));
                }
            }
        }
        ReflectionMut::Repeated(r) => {
            let arr = match value {
                json::Value::Array(els) => els,
                _ => {
                    return Err(err_msg("Unsupported json value for repeated field"));
                }
            };

            for value in arr {
                apply_json_value_to_reflection(r.reflect_add(), value, options)?;
            }
        }
        ReflectionMut::Message(r) => {
            let obj = match value {
                json::Value::Object(v) => v,
                _ => return Err(err_msg("Expected message to be encoded as an object")),
            };

            if maybe_parse_any_proto(r, obj, options)? {
                return Ok(());
            }

            parse_message(r, obj, options, false)?;
        }
        ReflectionMut::Enum(r) => {
            match value {
                json::Value::String(v) => {
                    r.assign_name(&v)?;
                }
                json::Value::Number(n) => {
                    let num = *n as EnumValue;

                    // Verify we had a lossless conversion.
                    if (num as f64) != *n {
                        return Err(err_msg("Json number can't be cast to an enum value"));
                    }

                    r.assign(num)?;
                }
                _ => {
                    return Err(err_msg("Unsupported json value for enum"));
                }
            }
        }
        ReflectionMut::Set(_) => todo!(),
    };

    Ok(())
}

fn maybe_parse_any_proto(
    r: &mut dyn MessageReflection,
    obj: &HashMap<String, json::Value>,
    options: &ParserOptions,
) -> Result<bool> {
    const ANY_TYPE_URL_FIELD_NUM: FieldNumber = 1; // Any::TYPE_URL_FIELD_NUM
    const ANY_VALUE_FIELD_NUM: FieldNumber = 2; // Any::VALUE_FIELD_NUM

    if r.type_url() != "type.googleapis.com/google.protobuf.Any" {
        return Ok(false);
    }

    let type_url = match obj.get("@type") {
        Some(json::Value::String(s)) => s,
        _ => return Err(err_msg("Any proto missing @type string")),
    };

    let message_factory = options
        .message_factory
        .clone()
        .ok_or_else(|| err_msg("Need a MessageFactory to parse Any protos"))?;

    let mut inner_message = message_factory
        .new_message(type_url.as_str())
        .ok_or_else(|| format_err!("Unknown message with type_url: {}", type_url))?;
    parse_message(inner_message.as_mut(), obj, options, true)?;

    // Merge back into the base object.

    let type_url_field = match r.field_by_number_mut(ANY_TYPE_URL_FIELD_NUM) {
        Some(ReflectionMut::String(s)) => s,
        _ => return Err(err_msg("Failed to get type_url field")),
    };

    *type_url_field = type_url.clone();

    let value_field = match r.field_by_number_mut(ANY_VALUE_FIELD_NUM) {
        Some(ReflectionMut::Bytes(v)) => v,
        _ => return Err(err_msg("Failed to get value field")),
    };

    value_field.extend_from_slice(&inner_message.serialize()?);

    Ok(true)
}

fn parse_message(
    r: &mut dyn MessageReflection,
    obj: &HashMap<String, json::Value>,
    options: &ParserOptions,
    skip_type_url: bool,
) -> Result<()> {
    for (key, value) in obj.iter() {
        if skip_type_url && key == "@type" {
            continue;
        }

        let num = r
            .field_number_by_name(key.as_str())
            .ok_or_else(|| format_err!("Unknown message field named: {}", key))?;

        if let json::Value::Null = value {
            continue;
        }

        let r = r.field_by_number_mut(num).unwrap();
        apply_json_value_to_reflection(r, value, options)?;
    }

    Ok(())
}

fn get_f64(value: &json::Value) -> Result<f64> {
    match value {
        json::Value::Number(v) => Ok(*v),
        json::Value::String(v) => {
            if v == "Infinity" {
                Ok(f64::INFINITY)
            } else if v == "-Infinity" {
                Ok(f64::NEG_INFINITY)
            } else if v == "NaN" {
                Ok(f64::NAN)
            } else {
                Ok(v.parse()?)
            }
        }
        _ => Err(err_msg("Unsupported json value for float/double")),
    }
}
