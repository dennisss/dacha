use base_error::*;
use cnc_monitor_proto::cnc::*;

use crate::protobuf_table::*;

/*
Tables:
- Machines
- Files
- ProgramRun

*/

pub const MACHINE_TABLE_TAG: MachineTable = MachineTable {};
pub const FILE_TABLE_TAG: FileTable = FileTable {};
pub const MEDIA_FRAGMENT_TABLE_TAG: MediaFragmentTable = MediaFragmentTable {};

pub struct MachineTable {}

impl ProtobufTableTag for MachineTable {
    type Message = MachineProto;

    fn table_name(&self) -> &str {
        "Machine"
    }

    fn indexed_keys(&self) -> Vec<ProtobufTableKey> {
        vec![ProtobufTableKey {
            index_name: None,
            fields: vec![MachineProto::ID_FIELD_NUM],
        }]
    }
}

pub struct FileTable {}

impl ProtobufTableTag for FileTable {
    type Message = FileProto;

    fn table_name(&self) -> &str {
        "File"
    }

    fn indexed_keys(&self) -> Vec<ProtobufTableKey> {
        vec![ProtobufTableKey {
            index_name: None,
            fields: vec![FileProto::ID_FIELD_NUM],
        }]
    }
}

/*
pub struct MediaStreamTable {}

impl ProtobufTableTag for MediaStreamTable {
    type Message = MediaStream;

    fn table_name(&self) -> &str {
        "MediaStream"
    }

    fn indexed_keys(&self) -> Vec<ProtobufTableKey> {
        vec![ProtobufTableKey {
            index_name: None,
            fields: vec![MediaStream::ID_FIELD_NUM],
        }]
    }
}
*/

pub struct MediaFragmentTable {}

impl ProtobufTableTag for MediaFragmentTable {
    type Message = MediaFragment;

    fn table_name(&self) -> &str {
        "MediaFragment"
    }

    fn indexed_keys(&self) -> Vec<ProtobufTableKey> {
        vec![ProtobufTableKey {
            index_name: None,
            fields: vec![
                MediaFragment::CAMERA_ID_FIELD_NUM,
                MediaFragment::START_TIME_FIELD_NUM,
            ],
        }]
    }
}

/*

ProgramRun
- machine_id
- start_time
- end_time
- state
- file_id

Event:
- Key
    - machine_id
    - time
- Value
    - type:
        PROGRAM_START
        PROGRAM_PLAY
        PROGRAM_PAUSE
        PROGRAM_DONE
        PRINT_LAYER




MetricValue:
- machine_id
- metric_name (T0)
- time
- metric_value (float)




*/
