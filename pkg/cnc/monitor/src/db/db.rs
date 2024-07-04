use std::collections::HashMap;

use base_error::*;
use datastore_meta_client::key_utils::find_short_successor;
use executor::sync::AsyncMutex;
use file::LocalPath;
use protobuf::reflection::{Reflection, ReflectionMut};
use protobuf::{FieldNumber, Message, MessageReflection, StaticMessage, TypedFieldNumber};
use sstable::db::WriteBatch;
use sstable::{db::SnapshotIteratorOptions, iterable::Iterable};
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::db::key::*;
use crate::db::table::*;

#[derive(Default)]
pub struct Query {
    any_of: Vec<QueryAllOf>,
}

impl Query {
    pub fn or(&mut self, all_of: QueryAllOf) -> &mut Self {
        self.any_of.push(all_of);
        self
    }
}

#[derive(Default)]
pub struct QueryAllOf {
    fields: HashMap<FieldNumber, Vec<QueryOperation>>,
}

impl QueryAllOf {
    pub fn and(&mut self, field: FieldNumber, op: QueryOperation) -> &mut Self {
        self.fields.entry(field).or_default().push(op);
        self
    }
}

pub enum QueryOperation {
    Eq(QueryValue),
    LessThan(QueryValue),
    LessThanOrEqual(QueryValue),
    GreaterThan(QueryValue),
    GreaterThanOrEqual(QueryValue),
}

pub enum QueryValue {
    U32(u32),
    U64(u64),
}

impl QueryValue {
    fn reflect<'a>(&'a self) -> Reflection<'a> {
        match self {
            QueryValue::U32(v) => Reflection::U32(v),
            QueryValue::U64(v) => Reflection::U64(v),
        }
    }
}

pub struct ProtobufDB {
    db: EmbeddedDB,
}

/*
TODO: A potential innefficiency with this approach is that protobufs would be used as an intermediate representation everywhere even if only a few fields are set.

Generalized queries:

- Have some 'message' + constraints.

- Eq(FieldNumber, Value)



Typical queries:
- List all: empty query
- Runs for machine:
    - Eq(machine_id, 123)

- Runs for a file
    - Eq(file_id, 445454)
    - Afterwards sort by run_id
        Smart way to do this would be to have the key be '[ file_id, run_id, machine_id ]'

- Video segments:
    - And
        - Eq(camera_id, 456)
        - GreaterThan(start_time, 1)
        - LessThan(start_time, 100)

- Metrics


&[
]

A query is effectively and OR of ANDs
- Each AND is basically one table scan

*/

impl ProtobufDB {
    pub async fn create(path: &LocalPath) -> Result<Self> {
        let mut options = EmbeddedDBOptions::default();
        options.create_if_missing = true;
        options.error_if_exists = false;

        let db = EmbeddedDB::open(path, options).await?;

        Ok(Self { db })
    }

    pub async fn list<Tag: ProtobufTableTag>(&self) -> Result<Vec<Tag::Message>> {
        // // TODO: Switch to using integer table ids.
        // let mut key_prefix = vec![];
        // KeyEncoder::encode_bytes(Tag::table_name().as_bytes(), &mut key_prefix);

        let mut query = Query::default();
        query.or(QueryAllOf::default());

        self.query::<Tag>(&query).await
    }

    /*
    Key format is:
    - [Table Id : varuint]
    - [Index Id : varuint]
    - [.. User Key Values ..]
    - [Column Family ID] - Only serialized if non-zero

    */

    pub async fn query<Tag: ProtobufTableTag>(&self, query: &Query) -> Result<Vec<Tag::Message>> {
        let snapshot = self.db.snapshot().await;

        let mut out = vec![];

        // TODO: Need to merge any clauses that refer to overlapping sets.
        for clause in &query.any_of {
            let primary_key_index = 0;
            let primary_key_config = &Tag::indexed_keys()[primary_key_index];

            let mut min_key = KeyBuilder::<Tag>::new(primary_key_index);
            let mut min_is_inclusive = true;

            let mut max_key = KeyBuilder::<Tag>::new(primary_key_index);
            let mut max_is_inclusive = true;

            let mut num_prefix_fields_matched = 0;

            /*
            TODO: First see if we can match against one or more indexes.
            - In the case of multiple indexes, we'd want to merge based on the primary keys.

            => Output will a stream of messages containing the primary keys (in sorted order)
                -> Will want to

            Can have the user provide a hint:
                - E.g. some fields we will allow checking via a scan. Others must require a
            */

            /*
            Greedy algorithm:
            - Find the index that matches to the most number of keys in the prefix

            - Find the first index that matches against at least one key.
            - Use that index to scan the primary key table and

            */

            /*
            Issue is that the number of encoded fields will

            */

            for field in primary_key_config.fields {
                let inverted = field.direction == Direction::Descending;

                let op = match clause.fields.get(&field.number.raw()) {
                    Some(v) => v,
                    None => break,
                };

                if op.len() == 0 {
                    break;
                }

                if op.len() == 1 {
                    if let QueryOperation::Eq(v) = &op[0] {
                        min_key.append(v.reflect());
                        max_key.append(v.reflect());
                        num_prefix_fields_matched += 1;
                        continue;
                    }
                }

                // TODO: Should also implement indexing of field presence
                // - e.g. only create a column if the secondary key are present.

                let mut got_min = false;
                let mut got_max = false;

                for op in op {
                    match op {
                        QueryOperation::Eq(v) => {
                            return Err(err_msg(
                                "Can't mix Eq with other ANDed operations on the same field",
                            ));
                        }
                        QueryOperation::LessThan(v) => {
                            if got_max {
                                return Err(err_msg("Multiple < or <= constraints on same field"));
                            }

                            got_max = true;

                            if !inverted {
                                max_key.append(v.reflect())?;
                                max_is_inclusive = false;
                            } else {
                                min_key.append(v.reflect())?;
                                min_is_inclusive = false;
                            }
                        }
                        QueryOperation::LessThanOrEqual(v) => {
                            if got_max {
                                return Err(err_msg("Multiple < or <= constraints on same field"));
                            }

                            got_max = true;

                            if !inverted {
                                max_key.append(v.reflect())?;
                                max_is_inclusive = true;
                            } else {
                                min_key.append(v.reflect())?;
                                min_is_inclusive = true;
                            }
                        }
                        QueryOperation::GreaterThan(v) => {
                            if got_min {
                                return Err(err_msg("Multiple > or >= constraints on same field"));
                            }

                            got_min = true;

                            if !inverted {
                                min_key.append(v.reflect())?;
                                min_is_inclusive = false;
                            } else {
                                max_key.append(v.reflect())?;
                                max_is_inclusive = false;
                            }
                        }
                        QueryOperation::GreaterThanOrEqual(v) => {
                            if got_min {
                                return Err(err_msg("Multiple > or >= constraints on same field"));
                            }

                            got_min = true;

                            if !inverted {
                                min_key.append(v.reflect())?;
                                min_is_inclusive = true;
                            } else {
                                max_key.append(v.reflect())?;
                                max_is_inclusive = true;
                            }
                        }
                    }
                }

                num_prefix_fields_matched += 1;
                break;
            }

            let mut min_key = min_key.finish();
            if !min_is_inclusive {
                min_key = find_short_successor(min_key);
            }

            let mut max_key = max_key.finish();
            if max_is_inclusive {
                max_key = find_short_successor(max_key);
            }

            // TODO: If we can infer a total ordering between all of the AnyOf clauses, then
            // we should reuse this iterator between them.
            let mut iter = snapshot.iter().await?;
            iter.seek(&min_key).await?;

            while let Some(entry) = iter.next().await? {
                // TODO: Use a proper comparator.
                if &entry.key >= &max_key {
                    break;
                }

                let data = match entry.value {
                    Some(v) => v,
                    None => continue,
                };

                let mut msg = Tag::Message::parse(&data)?;
                KeyBuilder::<Tag>::decode_key(
                    primary_key_config,
                    primary_key_index,
                    &entry.key,
                    &mut msg,
                )?;

                // TODO: Must use any left over fields as extra filters (that weren't
                // constrained by some other index).
                // ^ for all these fields, we also need to ensure that we have type checked the
                // reflection discriminant.

                out.push(msg);
            }
        }

        Ok(out)
    }

    pub fn new_transaction<'a>(&'a self) -> ProtobufDBTransaction<'a> {
        ProtobufDBTransaction {
            inst: self,
            write: WriteBatch::new(),
        }
    }

    /// Performs either an insert or update
    /// TODO: Rename to upsert?
    pub async fn insert<Tag: ProtobufTableTag>(&self, value: &Tag::Message) -> Result<()> {
        let mut txn = self.new_transaction();
        txn.insert::<Tag>(value);
        txn.commit().await
    }

    pub async fn remove<Tag: ProtobufTableTag>(&self, value: &Tag::Message) -> Result<()> {
        let mut txn = self.new_transaction();
        txn.remove::<Tag>(value);
        txn.commit().await
    }
}

pub struct ProtobufDBTransaction<'a> {
    inst: &'a ProtobufDB,
    write: WriteBatch,
}

impl<'a> ProtobufDBTransaction<'a> {
    /// Performs either an insert or update
    pub fn insert<Tag: ProtobufTableTag>(&mut self, value: &Tag::Message) -> Result<()> {
        // TODO: If we have secondary keys, then we need to retrieve the previous value
        // of the key and delete/update any stale keys.

        self.mutate_row::<Tag>(value, true)
    }

    pub fn remove<Tag: ProtobufTableTag>(&mut self, value: &Tag::Message) -> Result<()> {
        // TODO: Must look up the complete previous value.

        self.mutate_row::<Tag>(value, false)
    }

    pub async fn commit(self) -> Result<()> {
        self.inst.db.write(&self.write).await
    }

    fn mutate_row<Tag: ProtobufTableTag>(
        &mut self,
        value: &Tag::Message,
        insert: bool,
    ) -> Result<()> {
        for (key_index, key_config) in Tag::indexed_keys().iter().enumerate() {
            let key = KeyBuilder::<Tag>::message_key(key_index, value)?;

            if !insert {
                self.write.delete(&key);
                continue;
            }

            if key_index == 0 {
                if key_config.index_name.is_some() {
                    return Err(err_msg("First key must be the primary key"));
                }

                let mut key_value = value.clone();
                for field in key_config.fields {
                    key_value.clear_field_with_number(field.number.raw());
                }

                let value_bytes = key_value.serialize()?;

                self.write.put(&key, &value_bytes);
            } else {
                self.write.put(&key, &[]);
            }
        }

        Ok(())
    }
}
