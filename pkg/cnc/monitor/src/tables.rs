use base_error::*;
use cnc_monitor_proto::cnc::*;

use crate::db::*;

pub struct MachineTable {}

impl ProtobufTableTag for MachineTable {
    type Message = MachineProto;

    fn table_id() -> u32 {
        1
    }

    fn table_name() -> &'static str {
        "Machine"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey<Self::Message>] {
        &[ProtobufTableKey {
            index_name: None,
            fields: &[ProtobufKeyField {
                number: MachineProto::ID_FIELD_NUM,
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        }]
    }
}

pub struct FileTable {}

impl ProtobufTableTag for FileTable {
    type Message = FileProto;

    fn table_id() -> u32 {
        2
    }

    fn table_name() -> &'static str {
        "File"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey<Self::Message>] {
        &[ProtobufTableKey {
            index_name: None,
            fields: &[ProtobufKeyField {
                number: FileProto::ID_FIELD_NUM,
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        }]
    }
}

pub struct MediaFragmentTable {}

impl ProtobufTableTag for MediaFragmentTable {
    type Message = MediaFragment;

    fn table_id() -> u32 {
        3
    }

    fn table_name() -> &'static str {
        "MediaFragment"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey<Self::Message>] {
        &[ProtobufTableKey {
            index_name: None,
            fields: &[
                ProtobufKeyField {
                    number: MediaFragment::CAMERA_ID_FIELD_NUM,
                    direction: Direction::Ascending,
                    fixed_size: true,
                },
                ProtobufKeyField {
                    number: MediaFragment::START_TIME_FIELD_NUM,
                    direction: Direction::Descending,
                    fixed_size: true,
                },
            ],
        }]
    }
}

pub struct ProgramRunTable {}

impl ProtobufTableTag for ProgramRunTable {
    type Message = ProgramRun;

    fn table_id() -> u32 {
        4
    }

    fn table_name() -> &'static str {
        "ProgramRun"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey<Self::Message>] {
        &[
            ProtobufTableKey {
                index_name: None,
                fields: &[
                    ProtobufKeyField {
                        number: ProgramRun::MACHINE_ID_FIELD_NUM,
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                    ProtobufKeyField {
                        number: ProgramRun::RUN_ID_FIELD_NUM,
                        direction: Direction::Descending,
                        fixed_size: true,
                    },
                ],
            },
            ProtobufTableKey {
                index_name: Some("ByFile"),
                fields: &[
                    ProtobufKeyField {
                        number: ProgramRun::FILE_ID_FIELD_NUM,
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                    ProtobufKeyField {
                        number: ProgramRun::RUN_ID_FIELD_NUM,
                        direction: Direction::Descending,
                        fixed_size: true,
                    },
                    ProtobufKeyField {
                        number: ProgramRun::MACHINE_ID_FIELD_NUM,
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                ],
            },
        ]
    }
}

pub struct MetricSampleTable {}

impl ProtobufTableTag for MetricSampleTable {
    type Message = MetricSample;

    fn table_id() -> u32 {
        5
    }

    fn table_name() -> &'static str {
        "MetricSample"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey<Self::Message>] {
        &[ProtobufTableKey {
            index_name: None,
            fields: &[
                ProtobufKeyField {
                    number: MetricSample::RESOURCE_KEY_FIELD_NUM,
                    direction: Direction::Ascending,
                    fixed_size: true,
                },
                ProtobufKeyField {
                    number: MetricSample::TIMESTAMP_FIELD_NUM,
                    direction: Direction::Descending,
                    fixed_size: true,
                },
            ],
        }]
    }
}

// TODO: Use me.
pub struct TableSchemaTable {}

impl ProtobufTableTag for TableSchemaTable {
    type Message = TableSchema;

    fn table_id() -> u32 {
        1_000_000
    }

    fn table_name() -> &'static str {
        "TableSchema"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey<Self::Message>] {
        &[ProtobufTableKey {
            index_name: None,
            fields: &[ProtobufKeyField {
                number: TableSchema::TABLE_ID_FIELD_NUM,
                direction: Direction::Ascending,
                fixed_size: false,
            }],
        }]
    }
}
