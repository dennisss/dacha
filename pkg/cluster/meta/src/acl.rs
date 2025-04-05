use std::{collections::HashMap, sync::Arc};

use base_error::*;
use cluster_client::acl::checker::check_entity_allowed;
use cluster_client::acl::principal::{Principal, PrincipalSet};
use cluster_client::meta::KeyPrefixACLTable;
use cluster_client::ClusterServerHandlerData;
use common::format::format_bytes;
use common::hash::FastHasherBuilder;
use container_proto::cluster::KeyPrefixACLProto;
use db_txn::{ACLProcessor, EmbeddedDBStateMachineProcessor};
use db_txn_proto::db::txn::*;
use db_table::db::{ProtobufDB, ProtobufDBKeyValueDecoder};
use db_table::key_utils::key_range_prefix;
use executor::lock;
use executor::sync::AsyncRwLock;
use sstable::db::{Snapshot, WriteBatch};

use crate::view::View;

/*
Optimal ACL storage:

- BTreeMap of the ACLs based on prefix.
- First filter iterator to first byte and find the '[X]' prefix ACL
    - Then do a relative seek

- If the ACLs were just in a sorted list, this would be easy to implement.

- Long term, all key ranges would have a se

Assumption is that ACLs can fit in RAM

Mutatiosn will be infrequent so having a simpler tree with a single lock is probabl fine.


*/

/// Addon to the metastore that hooks into request processing to ensure ACL
/// restrictions.
pub struct KeyPrefixACLProcessor {
    zone: String,

    acl_table_decoder: ProtobufDBKeyValueDecoder<KeyPrefixACLTable>,

    state: AsyncRwLock<State>,
}

#[derive(Default)]
struct State {
    // TODO: If a resize is required, use a reader lock and clone
    acls: HashMap<Vec<u8>, KeyPrefixACL, FastHasherBuilder>,
}

#[derive(Debug)]
struct KeyPrefixACL {
    readers: Vec<Principal>,
    writers: Vec<Principal>,
}

impl KeyPrefixACLProcessor {
    pub fn new(zone: &str) -> Self {
        Self {
            zone: zone.to_string(),
            acl_table_decoder: ProtobufDBKeyValueDecoder::new(),
            state: AsyncRwLock::default(),
        }
    }

    async fn reload_db_impl(&self, snapshot: Snapshot) -> Result<()> {
        let db = ProtobufDB::new(Arc::new(View::new(snapshot)));

        let mut acls = HashMap::<Vec<u8>, KeyPrefixACL, FastHasherBuilder>::default();

        for proto in db.list::<KeyPrefixACLTable>().await? {
            acls.insert(proto.prefix().to_vec(), Self::proto_to_acl(&proto)?);
        }

        lock!(state <= self.state.write().await?, {
            state.acls = acls;
        });

        Ok(())
    }

    fn proto_to_acl(proto: &KeyPrefixACLProto) -> Result<KeyPrefixACL> {
        let mut readers = vec![];
        for s in proto.readers() {
            readers.push(Principal::parse(s)?);
        }

        let mut writers = vec![];
        for s in proto.writers() {
            writers.push(Principal::parse(s)?);
        }

        Ok(KeyPrefixACL { readers, writers })
    }

    async fn apply_change_impl(&self, change: &WriteBatch) -> Result<()> {
        for change in change.iter()? {
            let change = change?;

            let (msg, deleted) = match change {
                sstable::db::Write::Value { key, value } => {
                    (self.acl_table_decoder.decode_value(key, value)?, false)
                }
                sstable::db::Write::Deletion { key } => {
                    (self.acl_table_decoder.decode_deletion(key)?, true)
                }
            };

            let msg = match msg {
                Some(v) => v,
                None => continue,
            };

            // TODO: Only need to form this if !deleted.
            let new_rule = Self::proto_to_acl(&msg)?;

            // TODO: If the hash map needs to be resized, do that with a reader lock before
            // swapping the whole map.
            lock!(state <= self.state.write().await?, {
                if deleted {
                    state.acls.remove(msg.prefix());
                } else {
                    state.acls.insert(msg.prefix().to_vec(), new_rule);
                }
            });
        }

        Ok(())
    }

    async fn before_read_impl(
        &self,
        snapshot: &Snapshot,
        key_ranges: &[KeyRange],
        context: &rpc::ServerRequestContext,
    ) -> Result<()> {
        // TODO: Error up beforehand if we see empty rnages.

        self.check_key_prefix_acls(
            snapshot,
            key_ranges
                .iter()
                .map(|key_range| key_range_prefix(key_range.start_key(), key_range.end_key())),
            false,
            context,
        )
        .await
    }

    async fn before_execute_impl(
        &self,
        snapshot: &Snapshot,
        transaction: &Transaction,
        context: &rpc::ServerRequestContext,
    ) -> Result<()> {
        self.check_key_prefix_acls(
            snapshot,
            transaction
                .reads()
                .iter()
                .map(|key_range| key_range_prefix(key_range.start_key(), key_range.end_key())),
            false,
            context,
        )
        .await?;

        self.check_key_prefix_acls(
            snapshot,
            transaction.writes().iter().map(|write| write.key()),
            true,
            context,
        )
        .await?;

        // TODO: Must also validate that writes to ACL table rows are well formed.

        Ok(())
    }

    async fn check_key_prefix_acls<'a, T: Iterator<Item = &'a [u8]>>(
        &self,
        snapshot: &Snapshot,
        mut key_prefixes: T,
        writing: bool,
        context: &rpc::ServerRequestContext,
    ) -> Result<()> {
        let conn = ClusterServerHandlerData::from_rpc_context(context)?;

        let db = ProtobufDB::new(Arc::new(View::new(snapshot.clone())));

        let state = self.state.read().await?;

        // TODO: Can optimize since the key ranges should be sorted so most of the
        // prefix should be re-useable across checks.

        // (make sure that the request is validated prior to being given to us for both
        // Read and Execute)

        // NOTE: If a key rnage spans multiple prefixes, we can't check the ACLs
        for prefix in key_prefixes {
            // TODO: Error up beforehand if we see empty rnages.

            let mut allowed_principles = PrincipalSet::default();

            for i in 0..(prefix.len() + 1) {
                if let Some(acl) = state.acls.get(&prefix[..i]) {
                    if writing {
                        for p in &acl.writers {
                            allowed_principles.insert(p.clone());
                        }
                    } else {
                        for p in &acl.readers {
                            allowed_principles.insert(p.clone());
                        }
                    }
                }
            }

            if !check_entity_allowed(
                conn.peer.as_ref(),
                &allowed_principles,
                &self.zone,
                Some(&db),
            )
            .await?
            {
                /*
                println!("Known keys: {:?}", state.acls);

                println!(
                    "Reject ({} \"{}\"): Allowed: {}",
                    if writing { "WRITE" } else { "READ" },
                    format_bytes(prefix),
                    allowed_principles
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                */

                return Err(rpc::Status::permission_denied("Access denied to key range").into());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl EmbeddedDBStateMachineProcessor for KeyPrefixACLProcessor {
    async fn reload_db(&self, snapshot: Snapshot) -> Result<()> {
        self.reload_db_impl(snapshot).await
    }

    async fn apply_change(&self, change: &WriteBatch) -> Result<()> {
        self.apply_change_impl(change).await
    }
}

#[async_trait]
impl ACLProcessor for KeyPrefixACLProcessor {
    async fn before_read(
        &self,
        snapshot: &Snapshot,
        key_ranges: &[KeyRange],
        context: &rpc::ServerRequestContext,
    ) -> Result<()> {
        self.before_read_impl(snapshot, key_ranges, context).await
    }

    async fn before_execute(
        &self,
        snapshot: &Snapshot,
        transaction: &Transaction,
        context: &rpc::ServerRequestContext,
    ) -> Result<()> {
        self.before_execute_impl(snapshot, transaction, context)
            .await
    }
}
