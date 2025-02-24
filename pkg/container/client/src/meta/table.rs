use std::collections::HashSet;
use std::marker::PhantomData;

use builder_proto::builder::BundleBlobSpec;
use common::errors::*;
use datastore_meta_client::MetastoreClient;
use db_table::define_singleton_table;
use db_table::table::*;
use protobuf::{Enum, Message, StaticMessage};

use crate::proto::*;

pub struct JobMetadataTable {}

impl ProtobufTableTag for JobMetadataTable {
    type Message = JobMetadata;

    fn table_id() -> u32 {
        ClusterTableId::Job as u32
    }

    fn table_name() -> &'static str {
        "JobMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[
                    JobMetadata::SPEC_FIELD_NUM_RAW,
                    WorkerSpec::NAME_FIELD_NUM_RAW,
                ],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}

pub struct WorkerMetadataTable {}

impl ProtobufTableTag for WorkerMetadataTable {
    type Message = WorkerMetadata;

    fn table_id() -> u32 {
        ClusterTableId::Worker as u32
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
            ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                index_name: None,
                fields: &[NAME],
            },
            ProtobufTableKey {
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
            },
        ]
    }
}

pub struct WorkerStateMetadataTable {}

impl ProtobufTableTag for WorkerStateMetadataTable {
    type Message = WorkerStateMetadata;

    fn table_id() -> u32 {
        ClusterTableId::WorkerState as u32
    }

    fn table_name() -> &'static str {
        "WorkerStateMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[WorkerStateMetadata::WORKER_NAME_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}

pub struct BundleBlobMetadataTable {}

impl ProtobufTableTag for BundleBlobMetadataTable {
    type Message = BundleBlobMetadata;

    fn table_id() -> u32 {
        ClusterTableId::BundleBlob as u32
    }

    fn table_name() -> &'static str {
        "BundleBlobMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[
                    BundleBlobMetadata::SPEC_FIELD_NUM_RAW,
                    BundleBlobSpec::ID_FIELD_NUM_RAW,
                ],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}

pub struct NodeMetadataTable {}

impl ProtobufTableTag for NodeMetadataTable {
    type Message = NodeMetadata;

    fn table_id() -> u32 {
        ClusterTableId::Node as u32
    }

    fn table_name() -> &'static str {
        "NodeMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[NodeMetadata::ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        }]
    }
}

pub struct NodeSchedulingMetadataTable {}

impl ProtobufTableTag for NodeSchedulingMetadataTable {
    type Message = NodeSchedulingMetadata;

    fn table_id() -> u32 {
        ClusterTableId::NodeScheduling as u32
    }

    fn table_name() -> &'static str {
        "NodeSchedulingMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[NodeSchedulingMetadata::NODE_ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        }]
    }
}

define_singleton_table!(ZoneMetadataTable {
    message: ZoneMetadata,
    table_id: ClusterTableId::Zone as u32,
    table_name: "ZoneMetadata"
});

pub struct ObjectMetadataTable {}

impl ProtobufTableTag for ObjectMetadataTable {
    type Message = ObjectMetadata;

    fn table_id() -> u32 {
        ClusterTableId::Object as u32
    }

    fn table_name() -> &'static str {
        "ObjectMetadata"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            fields: &[ProtobufKeyField {
                path: &[ObjectMetadata::NAME_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}
