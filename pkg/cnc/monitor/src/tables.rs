use base_error::*;
use cnc_monitor_proto::cnc::*;
use db_table::{sparse_struct, table_id};

use crate::db::*;

pub struct MachineTable {}

impl ProtobufTableTag for MachineTable {
    type Message = MachineProto;

    fn table_id() -> u32 {
        table_id!(32)
    }

    fn table_name() -> &'static str {
        "Machine"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[MachineProto::ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        })]
    }
}

pub struct FileTable {}

impl ProtobufTableTag for FileTable {
    type Message = FileProto;

    fn table_id() -> u32 {
        table_id!(33)
    }

    fn table_name() -> &'static str {
        "File"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[ProtobufKeyField {
                path: &[FileProto::ID_FIELD_NUM_RAW],
                direction: Direction::Ascending,
                fixed_size: true,
            }],
        })]
    }
}

pub struct MediaFragmentTable {}

impl ProtobufTableTag for MediaFragmentTable {
    type Message = MediaFragment;

    fn table_id() -> u32 {
        table_id!(34)
    }

    fn table_name() -> &'static str {
        "MediaFragment"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[
                ProtobufKeyField {
                    path: &[MediaFragment::CAMERA_ID_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: true,
                },
                ProtobufKeyField {
                    path: &[MediaFragment::START_TIME_FIELD_NUM_RAW],
                    direction: Direction::Descending,
                    fixed_size: true,
                },
            ],
        })]
    }
}

pub struct ProgramRunTable {}

impl ProtobufTableTag for ProgramRunTable {
    type Message = ProgramRun;

    fn table_id() -> u32 {
        table_id!(35)
    }

    fn table_name() -> &'static str {
        "ProgramRun"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[
            sparse_struct!(ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                fields: &[
                    ProtobufKeyField {
                        path: &[ProgramRun::MACHINE_ID_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                    ProtobufKeyField {
                        path: &[ProgramRun::RUN_ID_FIELD_NUM_RAW],
                        direction: Direction::Descending,
                        fixed_size: true,
                    },
                ],
            }),
            sparse_struct!(ProtobufTableKey {
                index_id: 1,
                index_name: Some("ByFile"),
                fields: &[
                    ProtobufKeyField {
                        path: &[ProgramRun::FILE_ID_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                    ProtobufKeyField {
                        path: &[ProgramRun::RUN_ID_FIELD_NUM_RAW],
                        direction: Direction::Descending,
                        fixed_size: true,
                    },
                    ProtobufKeyField {
                        path: &[ProgramRun::MACHINE_ID_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                ],
            }),
        ]
    }
}

pub struct MetricSampleTable {}

impl ProtobufTableTag for MetricSampleTable {
    type Message = MetricSample;

    fn table_id() -> u32 {
        table_id!(36)
    }

    fn table_name() -> &'static str {
        "MetricSample"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            index_name: None,
            filter: None,
            fields: &[
                ProtobufKeyField {
                    path: &[MetricSample::RESOURCE_KEY_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: true,
                },
                ProtobufKeyField {
                    path: &[MetricSample::TIMESTAMP_FIELD_NUM_RAW],
                    direction: Direction::Descending,
                    fixed_size: true,
                },
            ],
        })]
    }
}

pub struct ProgramPreviewTable {}

impl ProtobufTableTag for ProgramPreviewTable {
    type Message = ProgramPreviewProto;

    fn table_id() -> u32 {
        table_id!(39)
    }

    fn table_name() -> &'static str {
        "ProgramPreview"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[
                ProtobufKeyField {
                    path: &[ProgramPreviewProto::FILE_ID_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: true,
                },
                ProtobufKeyField {
                    path: &[ProgramPreviewProto::CONFIG_HASH_FIELD_NUM_RAW],
                    direction: Direction::Ascending,
                    fixed_size: true,
                },
            ],
        })]
    }
}
