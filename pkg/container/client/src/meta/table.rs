use std::collections::HashSet;

use builder_proto::builder::BundleBlobSpec;
use common::errors::*;
use container_proto::cluster::*;
use db_table::table::*;
use db_table::table_id;
use db_table::{define_singleton_table, sparse_struct};
use protobuf::{Enum, Message, StaticMessage};

pub struct KeyPrefixACLTable {}

impl ProtobufTableTag for KeyPrefixACLTable {
    type Message = KeyPrefixACLProto;

    fn table_id() -> u32 {
        table_id!(4)
    }

    fn table_name() -> &'static str {
        "KeyPrefixACL"
    }

    fn single_index_table() -> bool {
        true
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            single_column_family: true,
            fields: &[ProtobufKeyField {
                path: &[KeyPrefixACLProto::PREFIX_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        })]
    }
}

pub struct GroupMembershipTable {}

impl ProtobufTableTag for GroupMembershipTable {
    type Message = GroupMembership;

    fn table_id() -> u32 {
        table_id!(5)
    }

    fn table_name() -> &'static str {
        "GroupMembership"
    }

    fn single_index_table() -> bool {
        true
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            single_column_family: true,
            fields: &[
                ProtobufKeyField {
                    path: &[GroupMembership::GROUP_NAME_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: false,
                },
                ProtobufKeyField {
                    path: &[GroupMembership::EXPANDS_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: false,
                },
                ProtobufKeyField {
                    path: &[GroupMembership::MEMBER_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: false,
                },
            ],
        })]
    }
}

pub struct JobMetadataTable {}

impl ProtobufTableTag for JobMetadataTable {
    type Message = JobMetadata;

    fn table_id() -> u32 {
        table_id!(16)
    }

    fn table_name() -> &'static str {
        "JobMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[
                    JobMetadata::SPEC_FIELD_NUM_RAW,
                    WorkerSpec::NAME_FIELD_NUM_RAW,
                ],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        })]
    }
}

// TODO: Because the proto is very large, it is currently not very efficient to
// just look up the 'assigned_node' field for this.
pub struct WorkerMetadataTable {}

impl ProtobufTableTag for WorkerMetadataTable {
    type Message = WorkerMetadata;

    fn table_id() -> u32 {
        table_id!(17)
    }

    fn table_name() -> &'static str {
        "WorkerMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        const NAME: ProtobufKeyField = ProtobufKeyField {
            path: &[
                WorkerMetadata::SPEC_FIELD_NUM_RAW,
                WorkerSpec::NAME_FIELD_NUM_RAW,
            ],
            direction: Direction::Ascending,
            fixed_size: false,
        };

        &[
            sparse_struct!(ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                fields: &[NAME],
            }),
            sparse_struct!(ProtobufTableKey {
                index_id: 1,
                index_name: Some("ByNode"),
                fields: &[
                    ProtobufKeyField {
                        path: &[WorkerMetadata::ASSIGNED_NODE_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                    NAME,
                ],
            }),
        ]
    }
}

pub struct WorkerStateMetadataTable {}

impl ProtobufTableTag for WorkerStateMetadataTable {
    type Message = WorkerStateMetadata;

    fn table_id() -> u32 {
        table_id!(18)
    }

    fn table_name() -> &'static str {
        "WorkerStateMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[WorkerStateMetadata::WORKER_NAME_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        })]
    }
}

pub struct BundleBlobMetadataTable {}

impl ProtobufTableTag for BundleBlobMetadataTable {
    type Message = BundleBlobMetadata;

    fn table_id() -> u32 {
        table_id!(19)
    }

    fn table_name() -> &'static str {
        "BundleBlobMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[
                    BundleBlobMetadata::SPEC_FIELD_NUM_RAW,
                    BundleBlobSpec::ID_FIELD_NUM_RAW,
                ],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        })]
    }
}

pub struct NodeMetadataTable {}

impl ProtobufTableTag for NodeMetadataTable {
    type Message = NodeMetadata;

    fn table_id() -> u32 {
        table_id!(20)
    }

    fn table_name() -> &'static str {
        "NodeMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[NodeMetadata::ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        })]
    }
}

pub struct NodeSchedulingMetadataTable {}

impl ProtobufTableTag for NodeSchedulingMetadataTable {
    type Message = NodeSchedulingMetadata;

    fn table_id() -> u32 {
        table_id!(21)
    }

    fn table_name() -> &'static str {
        "NodeSchedulingMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[NodeSchedulingMetadata::NODE_ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        })]
    }
}

pub struct ObjectMetadataTable {}

impl ProtobufTableTag for ObjectMetadataTable {
    type Message = ObjectMetadata;

    fn table_id() -> u32 {
        table_id!(22)
    }

    fn table_name() -> &'static str {
        "ObjectMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[ObjectMetadata::NAME_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        })]
    }
}

pub struct CertificateMetadataTable {}

impl ProtobufTableTag for CertificateMetadataTable {
    type Message = CertificateMetadata;

    fn table_id() -> u32 {
        table_id!(23)
    }

    fn table_name() -> &'static str {
        "CertificateMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        // TODO: Index by assigned node?
        &[
            sparse_struct!(ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                fields: &[ProtobufKeyField {
                    path: &[CertificateMetadata::SERIAL_NUMBER_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: false,
                }],
            }),
            sparse_struct!(ProtobufTableKey {
                index_id: 1,
                index_name: Some("Root"),
                filter: Some("root = TRUE"),
                fields: &[ProtobufKeyField {
                    path: &[CertificateMetadata::SERIAL_NUMBER_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: false,
                }],
            }),
        ]
    }
}

pub struct PrivateKeyMetadataTable {}

impl ProtobufTableTag for PrivateKeyMetadataTable {
    type Message = PrivateKeyMetadata;

    fn table_id() -> u32 {
        table_id!(24)
    }

    fn table_name() -> &'static str {
        "PrivateKeyMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[PrivateKeyMetadata::ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        })]
    }
}
