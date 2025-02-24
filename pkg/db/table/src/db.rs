use std::sync::Arc;

use base_error::*;
use db_kv::*;
use protobuf::{Message, MessageReflection, StaticMessage};

use crate::key::KeyBuilder;
use crate::key_utils::*;
use crate::query::*;
use crate::reflection::clear_field_by_path;
use crate::reflection::field_by_path;
use crate::reflection::field_by_path_mut;
use crate::table::*;

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

Finding a user;

- Search for 'name'
    -


Grabbing multiple users:
    - AnyOf
        Or( Eq(id, 1) )
        Or( Eq(id, 2) )


Operations in the graph:
    -


Posting list search

- Index with key [SearchToken, doc_id]
- To search for (A & B)
    - iterator over [A, ...]
    - iterator over [B, ...]
        - Each basically emits a primary key
        - ^ These need a way of skipping ahead to future primary keys
    - Merge streams

    [Search]


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
        let txn = self.new_transaction().await?;
        txn.insert::<Tag>(value).await?;
        txn.commit().await
    }

    pub async fn remove<Tag: ProtobufTableTag>(&self, value: &Tag::Message) -> Result<()> {
        let txn = self.new_transaction().await?;
        txn.remove::<Tag>(value).await?;
        txn.commit().await
    }
}

pub struct ProtobufDBTransaction<'a> {
    inst: &'a ProtobufDB,
    txn: Box<dyn KeyValueStoreTransaction + 'a>,
}

impl<'a> ProtobufDBTransaction<'a> {
    pub async fn query<Tag: ProtobufTableTag>(&self, query: &Query) -> Result<Vec<Tag::Message>> {
        // let snapshot = self.db.snapshot().await;

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


            First operation:
            - Scan range in the secondary index:
                - Output is a list of primary keys
                - Go through each of them and





            */

            for field in primary_key_config.fields {
                let inverted = field.direction == Direction::Descending;

                let op = match clause.fields.get(field.path) {
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
            let mut iter = self
                .txn
                .iter(KeyValueIteratorOptions {
                    start_key: min_key.into(),
                    end_key: max_key.into(),
                })
                .await?;

            while let Some(entry) = iter.next().await? {
                let mut msg = Tag::Message::parse(&entry.value)?;
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

    pub async fn read_index(&self) -> u64 {
        self.txn.read_index().await
    }

    pub async fn list<Tag: ProtobufTableTag>(&self) -> Result<Vec<Tag::Message>> {
        let mut query = Query::default();
        query.or(QueryAllOf::default());
        self.query::<Tag>(&query).await
    }

    /// Performs either an insert or update
    pub async fn insert<Tag: ProtobufTableTag>(&self, value: &Tag::Message) -> Result<()> {
        // TODO: If we have secondary keys, then we need to retrieve the previous value
        // of the key and delete/update any stale keys.

        self.mutate_row::<Tag>(value, true).await
    }

    pub async fn remove<Tag: ProtobufTableTag>(&self, value: &Tag::Message) -> Result<()> {
        // TODO: Must look up the complete previous value.

        self.mutate_row::<Tag>(value, false).await
    }

    pub async fn commit(self) -> Result<()> {
        self.txn.commit().await?;
        Ok(())
    }

    // TODO: This probably requires an extra lock over the keys to prevent
    // concurrent changes to the same row.
    async fn mutate_row<Tag: ProtobufTableTag>(
        &self,
        value: &Tag::Message,
        insert: bool,
    ) -> Result<()> {
        for (key_index, key_config) in Tag::indexed_keys().iter().enumerate() {
            let key = KeyBuilder::<Tag>::message_key(key_index, value)?;

            // TODO: Need to delete whenever we are unsure if the old value existed?
            if !insert {
                self.txn.delete(&key).await?;
                continue;
            }

            if key_index == 0 {
                // Primary key/index:
                // - 'key' is the concatenated primary index fields.
                // - 'value' is a protobuf with all fields in the message except those in the
                //   key.

                if key_config.index_name.is_some() {
                    return Err(err_msg("First key must be the primary key"));
                }

                let mut key_value = value.clone();
                for field in key_config.fields {
                    clear_field_by_path(&mut key_value, field.path)?;
                }

                let value_bytes = key_value.serialize()?;

                self.txn.put(&key, &value_bytes).await?;
            } else {
                // Secondary key/index:
                // - 'key'
                // - 'value' is all fields of the primary key that aren't included in this
                //   secondary key.

                if let Some(old_value) = old_value {
                    let old_key = KeyBuilder::message_key(Tag::table_id(), &key_config, old_value)?;
                    if old_key == key {
                        continue;
                    }

                    self.txn.delete(&old_key).await?;
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

                let value_bytes = secondary_value.serialize()?;

                self.txn.put(&key, &value_bytes).await?;
            }
        }

        Ok(())
    }
}
