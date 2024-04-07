// Wrapper around the RocksDB table properties object
//
// Compatible with RocksDB properties. See:
// https://github.com/facebook/rocksdb/blob/master/include/rocksdb/table_properties.h
// The names of each property is defined here:
// https://github.com/facebook/rocksdb/blob/50e470791dafb3db017f055f79323aef9a607e43/table/table_properties.cc

use std::collections::HashMap;

use common::errors::*;
use parsing::complete;
use protobuf::reflection::Reflection;
use protobuf::wire::{parse_varint, serialize_varint};
use protobuf::{reflection::ReflectionMut, FieldNumber, MessageReflection};
use protobuf_compiler_proto::dacha::KeyExtension;

use crate::table::TableProperties;

pub async fn parse_table_properties(mut data: HashMap<String, Vec<u8>>) -> Result<TableProperties> {
    let desc = protobuf::StaticDescriptorPool::global()
        .get_descriptor::<TableProperties>()
        .await?;

    let mut props = TableProperties::default();

    for field in desc.fields() {
        let key = field.proto().options().key()?;
        if key.is_empty() {
            return Err(err_msg("TableProperties field has no key"));
        }

        let key_str: &String = &*key;

        let value = {
            if let Some(v) = data.remove(key_str) {
                v
            } else {
                continue;
            }
        };

        match props
            .field_by_number_mut(field.proto().number() as FieldNumber)
            .unwrap()
        {
            ReflectionMut::U64(v) => {
                *v = complete(|input| parse_varint(input).map_err(|e| Error::from(e)))(&value)?.0
                    as u64
            }
            ReflectionMut::String(v) => {
                // TODO: Ensure that this is always utf-8
                *v = String::from_utf8(value.to_vec())?;
            }
            _ => return Err(err_msg("Unsupported properties field type")),
        }
    }

    Ok(props)
}

pub async fn serialize_table_properties(
    props: &TableProperties,
) -> Result<HashMap<String, Vec<u8>>> {
    let desc = protobuf::StaticDescriptorPool::global()
        .get_descriptor::<TableProperties>()
        .await?;

    let mut data = HashMap::default();

    for field in desc.fields() {
        let key = field
            .proto()
            .options()
            .key()
            .map_err(|e| format_err!("Failed to get the key: {}", e))?;
        if key.is_empty() {
            return Err(err_msg("TableProperties field has no key"));
        }

        let value = match props.field_by_number(field.proto().number() as FieldNumber) {
            Some(v) => v,
            // No value present.
            None => continue,
        };

        let mut serialized_value = vec![];

        match value {
            Reflection::U64(v) => {
                serialize_varint(*v, &mut serialized_value)?;
            }
            Reflection::String(v) => {
                serialized_value.extend_from_slice(v.as_bytes());
            }
            _ => return Err(err_msg("Unsupported properties field type")),
        }

        data.insert(key.clone(), serialized_value);
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[testcase]
    async fn serialize_only_present_properties() -> Result<()> {
        let mut props = TableProperties::default();

        let serialized = serialize_table_properties(&props).await?;
        assert_eq!(serialized, HashMap::default());
        assert_eq!(parse_table_properties(serialized).await?, props);

        props.set_raw_key_size(0u64);
        assert!(props.has_raw_key_size());

        let serialized = serialize_table_properties(&props).await?;
        assert_eq!(serialized, map!("rocksdb.raw.key.size" => &[0u8][..]));
        assert_eq!(parse_table_properties(serialized).await?, props);

        props.set_raw_value_size(22u64);

        let serialized = serialize_table_properties(&props).await?;
        assert_eq!(
            serialized,
            map!(
                "rocksdb.raw.key.size" => &[0u8][..],
                "rocksdb.raw.value.size" => &[22u8][..]
            )
        );
        assert_eq!(parse_table_properties(serialized).await?, props);

        Ok(())
    }
}
