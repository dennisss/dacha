// Utilities for setting up metastore ACLs.

use cluster_client::{acl::principal::Principal, meta::*, service::address::ServiceName};
use common::errors::*;
use container_proto::cluster::GroupMembership;
use container_proto::cluster::KeyPrefixACLProto;
use db_table::{
    db::{ProtobufDB, ProtobufDBTransaction},
    raw_query,
    table::ProtobufTableTag,
};

/// Grants a node the ability to write to its own NodeMetadataTable row in the
/// metastore.
pub async fn authorize_node(node_id: u64, zone: &str, db: &ProtobufDB) -> Result<()> {
    let q = raw_query!(NodeMetadataTable, "id = ?", node_id);
    let key = ProtobufDBTransaction::primary_key_prefix::<NodeMetadataTable>(&q)?;

    let mut proto = KeyPrefixACLProto::default();
    proto.set_prefix(key);
    proto.add_writers(Principal::Entity(ServiceName::for_node(zone, node_id)?).to_string());

    db.insert::<KeyPrefixACLTable>(&proto).await
}

/// TODO: This needs to support deleting any unneeded ACLs should we ever want
/// to change the ACL structure and re-apply it to the cluster.
pub async fn bootstrap_acls(zone: &str, db: &ProtobufDB) -> Result<()> {
    let mut txn = db.new_transaction().await?;

    for v in get_group_memberships(zone) {
        txn.put::<GroupMembershipTable>(&v).await?;
    }

    for v in get_table_acls(zone)? {
        txn.put::<KeyPrefixACLTable>(&v).await?;
    }

    txn.commit().await?;

    Ok(())
}

fn get_group_memberships(zone: &str) -> Vec<GroupMembership> {
    vec![
        {
            let mut proto = GroupMembership::default();
            proto.set_group_name("cluster-readers");
            proto.set_expands(true);
            proto.set_member(Principal::Pattern("**.job.*.cluster.internal".into()).to_string());
            proto
        },
        {
            let mut proto = GroupMembership::default();
            proto.set_group_name("cluster-readers");
            proto.set_expands(true);
            proto.set_member(Principal::Pattern("**.node.*.cluster.internal".into()).to_string());
            proto
        },
    ]
}

fn make_table_acl<Tag: ProtobufTableTag>(readers: &[&str], writers: &[&str]) -> KeyPrefixACLProto {
    let mut proto = KeyPrefixACLProto::default();
    proto.set_prefix(ProtobufDBTransaction::table_key_prefix::<Tag>());
    for r in readers {
        proto.add_readers(r.to_string());
    }
    for w in writers {
        proto.add_writers(w.to_string());
    }
    proto
}

fn get_table_acls(zone: &str) -> Result<Vec<KeyPrefixACLProto>> {
    let cluster_readers = Principal::Group {
        zone: zone.to_string(),
        name: "cluster-readers".into(),
    }
    .to_string();

    let manager_job = Principal::Entity(ServiceName::for_job(zone, "system.manager")?).to_string();

    let ca_job =
        Principal::Entity(ServiceName::for_job(zone, "system.cert-authority")?).to_string();

    // Allow the manager to change the ACLs for the WorkerStateMetadataTable.
    //
    // NOTE: This also has the unnecessary side effect of allowing the manager to
    // change the permissions for changing the permissions of the
    // WorkerStateMetadataTable table.
    let manager_writes_worker_acls = {
        let q = raw_query!(
            KeyPrefixACLTable,
            "prefix = ?",
            ProtobufDBTransaction::table_key_prefix::<WorkerStateMetadataTable>()
        );
        let key = ProtobufDBTransaction::primary_key_prefix::<KeyPrefixACLTable>(&q)?;

        let mut proto = KeyPrefixACLProto::default();
        proto.set_prefix(key);
        proto.add_writers(manager_job.clone());
        proto
    };

    Ok(vec![
        // Readers: Services need to read groups to check their own ACLs.
        // Writers:
        make_table_acl::<GroupMembershipTable>(&[&cluster_readers], &[]),
        // Readers: Need for service resolving.
        // Writers: Just the manager job
        //          WorkerStateMetadata will also have per-row ACLs allowing nodes to write.
        make_table_acl::<JobMetadataTable>(&[&cluster_readers], &[&manager_job]),
        make_table_acl::<WorkerMetadataTable>(&[&cluster_readers], &[&manager_job]),
        make_table_acl::<WorkerStateMetadataTable>(&[&cluster_readers], &[&manager_job]),
        manager_writes_worker_acls,
        // Just needs to be read/written by the manager job.
        make_table_acl::<NodeSchedulingMetadataTable>(&[&manager_job], &[&manager_job]),
        // Readers: Need for service resolving.
        // Writers: Individual rows can be written by the corresponding nodes.
        make_table_acl::<NodeMetadataTable>(&[&cluster_readers], &[]),
        // Readers: Need a subset of the certificates for CA registry population (the whole table
        //          contains no secrets anyway).
        // Writers: Just the CA job.
        make_table_acl::<CertificateMetadataTable>(&[&cluster_readers], &[&ca_job]),
        // Secrets
        make_table_acl::<PrivateKeyMetadataTable>(&[&ca_job], &[&ca_job]),
        // Due to containing password hashes, this is secret.
        make_table_acl::<UserTable>(&[&ca_job], &[&ca_job]),
        // Not secret information so just granting cluster wide access for simplicity.
        make_table_acl::<BundleBlobMetadataTable>(&[&cluster_readers], &[&manager_job]),
    ])
}
