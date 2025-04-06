use std::collections::HashMap;
use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::Arc;

use base_error::*;
use common::bytes::Bytes;
use common::const_default::StaticDefault;
use db_kv::*;
use executor::sync::AsyncRwLock;
use executor::sync::SyncMutex;
use protobuf::{Message, MessageReflection, StaticMessage};

use crate::key::KeyBuilder;
use crate::key_utils::*;
use crate::query::*;
use crate::query_parser::QueryBuilder;
use crate::reflection::clear_field_by_path;
use crate::reflection::field_by_path;
use crate::reflection::field_by_path_mut;
use crate::table::*;

/*
TODO: A potential innefficiency with this approach is that protobufs would be used as an intermediate representation everywhere even if only a few fields are set.

Remaining TODOs
- Remove some of the templating

- Ensure that the DB interface is fully cancel safe.


- Query node
    - Properties:
        - Set of field numbers generated in the output stream
    - Output:
        - Stream of Box<dyn Message>
            - Each only has some sparse set of fields actually populated based on what is in the
    - Inputs:
        - N streams of


Standard nodes:

- Basic Lookup on Primary KEy
    - InputAttributes
        - IndexKey defininition

- StaticIterate
    - Input Attributes:
        - IndexedKey definition
        - List of key ranges over which to iterate
    - Output:
        - Stream of messages
    - What is does:
        - Read next row
        - Parse the value
        - Inject any key value.
        - Output the message

- LookupKeys
    - Input Attributes
        - IndexKey definition
    - Inputs:
        - Stream of messages containing the keys that we need to look up.
        - Each message corresponds to one key.

- Filter:
    - Given list of

*/

/*
TODO: Lots of validation needs to happen on the 'Tag':
- There is at least one indexed key and it is the primary key.
- Index keys are ordered
- No duplication of ids
- Primary key id is correct and has no name.
- Fields actually exist in the message and are indexable.
- No other duplicate table in the database.

*/

/*
struct TableIndexIterator<Tag: ProtobufTableTag + 'static> {
    key_config: &'static ProtobufTableKey,
    min_key: Vec<u8>,
    max_key: Vec<u8>,
}

impl<Tag: ProtobufTableTag + 'static> TableIndexIterator<Tag> {
    async fn next(&self) -> Result<Option<Tag::Message>> {
        todo!()
    }
}
*/

pub struct ProtobufDB {
    store: Arc<dyn KeyValueStore>,
}

impl ProtobufDB {
    pub fn new(store: Arc<dyn KeyValueStore>) -> Self {
        Self { store }
    }

    pub async fn new_transaction<'a>(&'a self) -> Result<ProtobufDBTransaction<'a>> {
        Ok(ProtobufDBTransaction {
            inst: self,
            txn: self.store.new_transaction().await?,
            poisoned: false,
        })
    }

    pub async fn list<Tag: ProtobufTableTag>(&self) -> Result<Vec<Tag::Message>> {
        let mut query = Query::default();
        query.or(QueryAllOf::default());

        self.query::<Tag>(&query).await
    }

    pub async fn query<Tag: ProtobufTableTag>(&self, query: &Query) -> Result<Vec<Tag::Message>> {
        let txn = self.new_transaction().await?;
        txn.query::<Tag>(query).await
    }

    /// Performs either an insert or update
    /// TODO: Rename to upsert?
    pub async fn insert<Tag: ProtobufTableTag>(&self, value: &Tag::Message) -> Result<()> {
        let mut txn = self.new_transaction().await?;
        txn.put::<Tag>(value).await?;
        txn.commit().await
    }

    pub async fn remove<Tag: ProtobufTableTag>(&self, value: &Tag::Message) -> Result<()> {
        let mut txn = self.new_transaction().await?;
        txn.remove::<Tag>(value).await?;
        txn.commit().await
    }
}

/// NOTE: The transaction must be mutable to make a write primarily because we
/// want to avoid having concurrent writes to conflicting rows of a table mess
/// each other up (e.g. overwrite each other's unique secondary keys).
pub struct ProtobufDBTransaction<'a> {
    inst: &'a ProtobufDB,

    txn: Box<dyn KeyValueStoreTransaction + 'a>,

    // If true, then a write operation did not complete successfully so this transaction must be
    // scraped.
    poisoned: bool,
}

#[derive(Debug)]
struct QueryIndexKeyRange {
    start_key: Vec<u8>,
    end_key: Vec<u8>,
    cost: u64,
}

impl<'a> ProtobufDBTransaction<'a> {
    // TODO: Make this code less templated to reduce the code size.
    pub async fn query<Tag: ProtobufTableTag>(&self, query: &Query) -> Result<Vec<Tag::Message>> {
        // TODO: Ideally perform filtering on the serialized protobufs to avoid having
        // to deserialize the full message to filter it out. To make this efficient, we
        // should ideally be only serializing in sorted field number order (so we just
        // need to merge two lists to filter). Naturally we can fully optimize this out
        // if we are dealing with

        self.check_readable()?;

        let mut out = vec![];
        let mut keys = HashSet::new();
        let mut callback = |key: Bytes, message: Tag::Message| -> Result<()> {
            // TODO: Don't need to compare to any fields already matched by the key range.
            if !query.matches(&message)? {
                return Ok(());
            }

            // Dedup.
            if query.any_of.len() > 1 && !keys.insert(key) {
                return Ok(());
            }

            out.push(message);
            Ok(())
        };

        /*
        TODO: Also attempt to execute the entire query using one of the indexes (subtract the filter and extract out common fields out of each clause)
        */

        // TODO: Need to merge any clauses that refer to overlapping sets.
        // TODO: Parallelize the clauses after we do query planning.
        for clause in &query.any_of {
            let mut index_keys = vec![];
            for key_config in Tag::indexed_keys() {
                index_keys.push((
                    key_config,
                    Self::get_index_key(
                        clause,
                        Tag::table_id(),
                        key_config,
                        Tag::Message::static_default(),
                    )?,
                ));
            }

            let (best_index_key_config, best_index_key) =
                index_keys.into_iter().min_by_key(|(_, k)| k.cost).unwrap();

            if best_index_key_config.index_id == PRIMARY_KEY_ID {
                self.query_with_primary_key::<Tag, _>(
                    best_index_key_config,
                    best_index_key,
                    &mut callback,
                )
                .await?;
            } else {
                self.query_with_secondary_key::<Tag, _>(
                    best_index_key_config,
                    best_index_key,
                    &mut callback,
                )
                .await?;
            }
        }

        Ok(out)
    }

    fn get_index_key(
        mut clause: &QueryAllOf,
        table_id: u32,
        key_config: &ProtobufTableKey,
        default_message: &dyn MessageReflection,
    ) -> Result<QueryIndexKeyRange> {
        let mut min_key = KeyBuilder::new(table_id, key_config, default_message);
        let mut min_is_inclusive = true;

        let mut max_key = KeyBuilder::new(table_id, key_config, default_message);
        let mut max_is_inclusive = true;

        let mut num_prefix_fields_matched = 0;

        let mut cost = 1;

        let filter_query = match key_config.filter {
            Some(v) => Some(QueryBuilder::create(v)?.build(default_message)?),
            None => None,
        };

        let filtered_clause = match filter_query {
            Some(v) => clause.subtract(&v),
            None => None,
        };

        let mut clause = clause;
        if let Some(c) = &filtered_clause {
            clause = c;
        }

        for field in key_config.fields {
            let inverted = field.direction == Direction::Descending;

            let cmps = match clause.fields.get(field.path) {
                Some(v) => v,
                None => break,
            };

            if cmps.len() == 0 {
                break;
            }

            if cmps.len() == 1 {
                if cmps[0].op == QueryOp::Eq {
                    min_key.append(cmps[0].rhs.reflect());
                    max_key.append(cmps[0].rhs.reflect());
                    num_prefix_fields_matched += 1;
                    continue;
                }
            }

            let mut got_min = false;
            let mut got_max = false;

            for cmp in cmps {
                match cmp.op {
                    QueryOp::Eq => {
                        return Err(err_msg(
                            "Can't mix Eq with other ANDed operations on the same field",
                        ));
                    }
                    QueryOp::LessThan | QueryOp::LessThanOrEqual => {
                        let inclusive = cmp.op == QueryOp::LessThanOrEqual;

                        if got_max {
                            return Err(err_msg("Multiple < or <= constraints on same field"));
                        }

                        got_max = true;

                        if !inverted {
                            max_key.append(cmp.rhs.reflect())?;
                            max_is_inclusive = inclusive;
                        } else {
                            min_key.append(cmp.rhs.reflect())?;
                            min_is_inclusive = inclusive;
                        }
                    }
                    QueryOp::GreaterThan | QueryOp::GreaterThanOrEqual => {
                        let inclusive = cmp.op == QueryOp::GreaterThanOrEqual;

                        if got_min {
                            return Err(err_msg("Multiple > or >= constraints on same field"));
                        }

                        got_min = true;

                        if !inverted {
                            min_key.append(cmp.rhs.reflect())?;
                            min_is_inclusive = inclusive;
                        } else {
                            max_key.append(cmp.rhs.reflect())?;
                            max_is_inclusive = inclusive;
                        }
                    }
                }
            }

            num_prefix_fields_matched += 1;

            if num_prefix_fields_matched != clause.fields.len() {
                cost *= 2;
            }

            break;
        }

        for _ in 0..(clause.fields.len() - num_prefix_fields_matched) {
            // TODO: USe '3' if we are partially reducing the cardinality with the 'filter'
            cost *= 4;
        }

        if key_config.index_id != PRIMARY_KEY_ID {
            cost += 1;
        }

        let mut min_key = min_key.finish();
        if !min_is_inclusive {
            min_key = find_short_successor(min_key);
        }

        let mut max_key = max_key.finish();
        if max_is_inclusive {
            max_key = find_short_successor(max_key);
        }

        Ok(QueryIndexKeyRange {
            start_key: min_key,
            end_key: max_key,
            cost,
        })
    }

    async fn query_with_primary_key<
        Tag: ProtobufTableTag,
        F: FnMut(Bytes, Tag::Message) -> Result<()>,
    >(
        &self,
        index_key_config: &ProtobufTableKey,
        index_key: QueryIndexKeyRange,
        callback: &mut F,
    ) -> Result<()> {
        // TODO: If we can infer a total ordering between all of the AnyOf clauses, then
        // we should reuse this iterator between them.
        let mut iter = self
            .txn
            .iter(KeyValueIteratorOptions {
                start_key: index_key.start_key.into(),
                end_key: index_key.end_key.into(),
            })
            .await?;

        while let Some(entry) = iter.next().await? {
            let mut msg = Tag::Message::parse(&entry.value)?;
            KeyBuilder::decode_key(Tag::table_id(), index_key_config, &entry.key, &mut msg)?;

            callback(entry.key, msg)?;
        }

        Ok(())
    }

    async fn query_with_secondary_key<
        Tag: ProtobufTableTag,
        F: FnMut(Bytes, Tag::Message) -> Result<()>,
    >(
        &self,
        index_key_config: &ProtobufTableKey,
        index_key: QueryIndexKeyRange,
        callback: &mut F,
    ) -> Result<()> {
        let mut iter = self
            .txn
            .iter(KeyValueIteratorOptions {
                start_key: index_key.start_key.into(),
                end_key: index_key.end_key.into(),
            })
            .await?;

        let primary_key_config = Tag::indexed_keys()
            .iter()
            .find(|k| k.index_id == PRIMARY_KEY_ID)
            .unwrap();

        while let Some(secondary_entry) = iter.next().await? {
            // NOTE: This will only contain the full primary key.
            let mut secondary_msg = Tag::Message::parse(&secondary_entry.value)?;
            KeyBuilder::decode_key(
                Tag::table_id(),
                index_key_config,
                &secondary_entry.key,
                &mut secondary_msg,
            )?;

            // TODO: Dedup this code with get().

            let primary_key =
                KeyBuilder::message_key(Tag::table_id(), primary_key_config, &secondary_msg)?;

            // TODO: There's some opportunity for optimization here since if the entries are
            // sorted by primary key, we should be able to get higher performance than doing
            // amny single row lookups by sharing an iterator on the db.

            let entry = self
                .get_one_row(&primary_key)
                .await?
                .ok_or_else(|| err_msg("Missing value referenced in secondary key"))?;

            let mut msg = Tag::Message::parse(&entry.value)?;
            KeyBuilder::decode_key(Tag::table_id(), primary_key_config, &entry.key, &mut msg)?;

            callback(entry.key, msg)?;
        }

        Ok(())
    }

    async fn get_one_row(&self, key: &[u8]) -> Result<Option<KeyValueEntry>> {
        let (start_key, end_key) = single_key_range(key);
        let mut iter = self
            .txn
            .iter(KeyValueIteratorOptions { start_key, end_key })
            .await?;
        iter.next().await
    }

    pub async fn read_index(&self) -> u64 {
        self.txn.read_index().await
    }

    pub async fn list<Tag: ProtobufTableTag>(&self) -> Result<Vec<Tag::Message>> {
        let mut query = Query::default();
        query.or(QueryAllOf::default());
        self.query::<Tag>(&query).await
    }

    /// Gets the row that has the same primary key as the given message.
    pub async fn get<Tag: ProtobufTableTag>(
        &self,
        value: &Tag::Message,
    ) -> Result<Option<Tag::Message>> {
        self.check_readable()?;

        let primary_key_config = Tag::indexed_keys()
            .iter()
            .find(|k| k.index_id == PRIMARY_KEY_ID)
            .unwrap();
        let primary_key = KeyBuilder::message_key(Tag::table_id(), primary_key_config, value)?;

        let entry = match self.get_one_row(&primary_key).await? {
            Some(v) => v,
            None => return Ok(None),
        };

        let mut msg = Tag::Message::parse(&entry.value)?;
        KeyBuilder::decode_key(Tag::table_id(), primary_key_config, &entry.key, &mut msg)?;

        Ok(Some(msg))
    }

    /// Performs either an insert or update
    pub async fn put<Tag: ProtobufTableTag>(&mut self, value: &Tag::Message) -> Result<()> {
        // If we support secondary keys,
        // - We need to look up the old value of the row,
        // - For each index/secondary key
        //   - If the old key value == new key value,
        //     - continue
        //   - If the index is a 'unique index' (doesn't have the full primary key in
        //     the key fields),
        //     - Look up the value of the secondary key in the database and return an
        //       error if it already exists.
        //   - Else delete old key value and insert the new one.
        //
        // Finally we need to insert the value into the primary key.

        self.check_readable()?;

        let mut old_value = None;
        if Tag::indexed_keys().len() > 1 {
            old_value = self.get::<Tag>(value).await?;
        }

        self.start_critical_section()?;
        self.put_impl::<Tag>(value, old_value.as_ref()).await?;
        self.end_critical_section();

        Ok(())
    }

    pub async fn remove<Tag: ProtobufTableTag>(&mut self, mut value: &Tag::Message) -> Result<()> {
        // If we support secondary keys,
        // - We need to look up the old value of the row and if the row exists, we will
        //   remove the row and all secondary keys.
        // Else,
        // - We just blindly trigger a deletion (assumption is that this is cheaper than
        //   looking up the value since the caller probably already has advance
        //   knowledge about the value existing).

        self.check_readable()?;

        let mut old_value = None;
        if Tag::indexed_keys().len() > 1 {
            old_value = self.get::<Tag>(value).await?;

            if let Some(v) = &old_value {
                value = v;
            } else {
                // Row does not exist so no point in removing it.
                return Ok(());
            }
        }

        self.start_critical_section()?;

        for key_config in Tag::indexed_keys() {
            let filter_query = match key_config.filter {
                Some(v) => Some(QueryBuilder::create(v)?.build(Tag::Message::static_default())?),
                None => None,
            };

            let indexed = match filter_query {
                Some(v) => v.matches(value)?,
                None => true,
            };

            if !indexed {
                continue;
            }

            let key = KeyBuilder::message_key(Tag::table_id(), key_config, value)?;
            self.txn.delete(&key).await?;
        }

        self.end_critical_section();

        Ok(())
    }

    fn check_readable(&self) -> Result<()> {
        if self.poisoned {
            return Err(executor::sync::PoisonError::MutationCancelled.into());
        }

        Ok(())
    }

    fn start_critical_section(&mut self) -> Result<()> {
        self.check_readable()?;
        self.poisoned = true;
        Ok(())
    }

    fn end_critical_section(&mut self) {
        self.poisoned = false;
    }

    pub async fn commit(mut self) -> Result<()> {
        self.start_critical_section()?;
        self.txn.commit().await?;
        Ok(())
    }

    async fn put_impl<Tag: ProtobufTableTag>(
        &mut self,
        value: &Tag::Message,
        old_value: Option<&Tag::Message>,
    ) -> Result<()> {
        // TODO: Need to verify this is the correct key.
        let primary_key_config = &Tag::indexed_keys()[0];

        for key_config in Tag::indexed_keys() {
            let key = KeyBuilder::message_key(Tag::table_id(), key_config, value)?;

            if key_config.index_id == PRIMARY_KEY_ID {
                // Primary key/index:
                // - 'key' is the concatenated primary index fields.
                // - 'value' is a protobuf with all fields in the message except those in the
                //   key.

                if key_config.index_name.is_some() {
                    return Err(err_msg("Primary key can't have a custom name"));
                }

                if key_config.filter.is_some() {
                    return Err(err_msg("Primary key can't have a custom name"));
                }

                let mut key_value = value.clone();
                for field in key_config.fields {
                    clear_field_by_path(&mut key_value, field.path)?;
                }

                let mut value_bytes = vec![];
                key_value.serialize_to(&protobuf::SerializeOptions::deterministic(), &mut value_bytes)?;

                self.txn.put(&key, &value_bytes).await?;
            } else {
                // Secondary key/index:
                // - 'key'
                // - 'value' is all fields of the primary key that aren't included in this
                //   secondary key.

                // TODO: If the filter query implies a field's value, we don't need to store it
                // in the 'key'/'value'.

                let filter_query = match key_config.filter {
                    Some(v) => {
                        Some(QueryBuilder::create(v)?.build(Tag::Message::static_default())?)
                    }
                    None => None,
                };

                let new_indexed = match filter_query.as_ref() {
                    Some(v) => v.matches(value)?,
                    None => true,
                };

                if let Some(old_value) = old_value {
                    let old_indexed = match filter_query.as_ref() {
                        Some(v) => v.matches(old_value)?,
                        None => true,
                    };

                    if old_indexed {
                        let old_key =
                            KeyBuilder::message_key(Tag::table_id(), &key_config, old_value)?;

                        // NOTE: We don't need to compare the values since we currently only ever
                        // store the primary key which can't change.
                        if old_key == key && new_indexed {
                            continue;
                        }

                        self.txn.delete(&old_key).await?;
                    }
                }

                if !new_indexed {
                    continue;
                }

                let mut secondary_value = Tag::Message::default();

                let primary_key_config = &Tag::indexed_keys()[0];

                let mut contains_all_primary_key_fields = true;

                for primary_key_field in primary_key_config.fields {
                    let in_secondary_key = key_config
                        .fields
                        .iter()
                        .find(|f| f.path == primary_key_field.path)
                        .is_some();

                    if !in_secondary_key {
                        field_by_path_mut(&mut secondary_value, primary_key_field.path)?
                            .clone_from(field_by_path(value, primary_key_field.path)?)?;
                        contains_all_primary_key_fields = false;
                    }
                }

                if !contains_all_primary_key_fields {
                    if let Some(_) = self.get_one_row(&key).await? {
                        return Err(format_err!(
                            "Unique constraint violated on index {}::{}",
                            Tag::table_name(),
                            key_config.index_name.unwrap_or("?")
                        ));
                    }
                }

                // TODO: This can usually be optimized since we can skip attempting to serialize any fields not in the primary key.
                let mut value_bytes = vec![];
                secondary_value.serialize_to(&protobuf::SerializeOptions::deterministic(), &mut value_bytes)?;

                self.txn.put(&key, &value_bytes).await?;
            }
        }

        Ok(())
    }

    /// Gets a key which is the prefix of all data in a table (and not the
    /// prefix of any data in any other table).
    pub fn table_key_prefix<Tag: ProtobufTableTag>() -> Vec<u8> {
        KeyBuilder::table_prefix(Tag::table_id())
    }

    pub fn primary_key_prefix<Tag: ProtobufTableTag>(query: &Query) -> Result<Vec<u8>> {
        if query.any_of.len() != 1 {
            return Err(err_msg(
                "Expected just one any_of clause to match the primary key",
            ));
        }

        let key = Self::get_index_key(
            &query.any_of[0],
            Tag::table_id(),
            &Tag::indexed_keys()[0],
            Tag::Message::static_default(),
        )?;

        if key.cost != 1 {
            return Err(err_msg("Query can not be fully matched by the primary key"));
        }

        Ok(key.start_key)
    }
}

/// Helper to decode raw key-value pairs containing primary key values from the
/// underlying database.
pub struct ProtobufDBKeyValueDecoder<Tag> {
    prefix: Vec<u8>,
    tag: PhantomData<Tag>,
}

impl<Tag: ProtobufTableTag> ProtobufDBKeyValueDecoder<Tag> {
    pub fn new() -> Self {
        // TODO: Avoid assuming that the first index is the primary key.
        let prefix = KeyBuilder::new(
            Tag::table_id(),
            &Tag::indexed_keys()[0],
            Tag::Message::static_default(),
        )
        .finish();

        Self {
            prefix,
            tag: PhantomData,
        }
    }

    pub fn decode_deletion(&self, key: &[u8]) -> Result<Option<Tag::Message>> {
        if !key.starts_with(&self.prefix) {
            return Ok(None);
        }

        let mut msg = Tag::Message::default();
        KeyBuilder::decode_key(Tag::table_id(), &Tag::indexed_keys()[0], &key, &mut msg)?;

        Ok(Some(msg))
    }

    pub fn decode_value(&self, key: &[u8], value: &[u8]) -> Result<Option<Tag::Message>> {
        if !key.starts_with(&self.prefix) {
            return Ok(None);
        }

        let mut msg = Tag::Message::parse(value)?;
        KeyBuilder::decode_key(Tag::table_id(), &Tag::indexed_keys()[0], &key, &mut msg)?;

        Ok(Some(msg))
    }
}
