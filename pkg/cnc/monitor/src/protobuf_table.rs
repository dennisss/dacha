use base_error::*;
use datastore_meta_client::key_encoding::KeyEncoder;
use executor::sync::AsyncMutex;
use file::LocalPath;
use protobuf::reflection::Reflection;
use protobuf::{FieldNumber, Message, MessageReflection, StaticMessage};
use sstable::{db::SnapshotIteratorOptions, iterable::Iterable};
use sstable::{EmbeddedDB, EmbeddedDBOptions};

const TABLE_SCHEMAS_TABLE_ID: usize = 1000;

pub trait ProtobufTableTag {
    type Message: protobuf::StaticMessage;

    fn table_name(&self) -> &str;

    /// Lists all fields that are present in the primary key.
    fn indexed_keys(&self) -> Vec<ProtobufTableKey>;
}

pub struct ProtobufTableKey {
    /// None implies this is the primary key
    pub index_name: Option<String>,
    pub fields: Vec<FieldNumber>,
}

pub struct ProtobufDB {
    db: EmbeddedDB,
}

impl ProtobufDB {
    pub async fn create(path: &LocalPath) -> Result<Self> {
        let mut options = EmbeddedDBOptions::default();
        options.create_if_missing = true;
        options.error_if_exists = false;

        let db = EmbeddedDB::open(path, options).await?;

        Ok(Self { db })
    }

    pub async fn list<Tag: ProtobufTableTag>(&self, tag: &Tag) -> Result<Vec<Tag::Message>> {
        // TODO: Switch to using integer table ids.
        let mut key_prefix = vec![];
        KeyEncoder::encode_bytes(tag.table_name().as_bytes(), &mut key_prefix);

        let snapshot = self.db.snapshot().await;

        let mut iter = snapshot.iter().await?;
        iter.seek(&key_prefix).await?;

        let mut out = vec![];
        while let Some(entry) = iter.next().await? {
            if !entry.key.starts_with(&key_prefix) {
                break;
            }

            let data = match entry.value {
                Some(v) => v,
                None => continue,
            };

            let msg = Tag::Message::parse(&data)?;
            out.push(msg);
        }

        Ok(out)
    }

    /// Performs either an insert or update
    pub async fn insert<Tag: ProtobufTableTag>(
        &self,
        tag: &Tag,
        value: &Tag::Message,
    ) -> Result<()> {
        let key = self.create_key(tag, value)?;
        let value = value.serialize()?;
        self.db.set(&key, &value).await?;

        Ok(())
    }

    pub async fn remove<Tag: ProtobufTableTag>(
        &self,
        tag: &Tag,
        value: &Tag::Message,
    ) -> Result<()> {
        let key = self.create_key(tag, value)?;
        self.db.delete(&key).await
    }

    fn create_key<Tag: ProtobufTableTag>(
        &self,
        tag: &Tag,
        value: &Tag::Message,
    ) -> Result<Vec<u8>> {
        let primary_key_def = tag
            .indexed_keys()
            .into_iter()
            .find(|v| v.index_name.is_none())
            .ok_or_else(|| err_msg("No primary key index defined"))?;

        let mut key = vec![];
        KeyEncoder::encode_bytes(tag.table_name().as_bytes(), &mut key);

        for field in primary_key_def.fields {
            // TODO: Need to switch reflection to return fields even if they have no field
            // presence.
            let r: Reflection = value
                .field_by_number(field)
                .ok_or_else(|| err_msg("Missing index field value"))?;

            match r {
                // TODO: Use encode_end_bytes if it is the last field.
                Reflection::String(v) => {
                    KeyEncoder::encode_bytes(v.as_bytes(), &mut key);
                }
                Reflection::Bytes(v) => {
                    KeyEncoder::encode_bytes(v, &mut key);
                }
                // TODO: Detect stuff like fixed32 and appropriately use fixed encoding here too.
                Reflection::U32(v) => KeyEncoder::encode_varuint(*v as u64, false, &mut key),
                Reflection::U64(v) => KeyEncoder::encode_varuint(*v, false, &mut key),
                // Reflection::I32(v) => ,
                // Reflection::I64(_) => todo!(),
                // Reflection::Bool(_) => todo!(),
                _ => {
                    return Err(err_msg("Index contains un-indexable field"));
                }
            }
        }

        Ok(key)
    }
}
