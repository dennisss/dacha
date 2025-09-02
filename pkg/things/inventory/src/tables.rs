use inventory_proto::inventory::*;
use db_table::*;

pub struct PartTable {}

impl ProtobufTableTag for PartTable {
    type Message = Part;

    fn table_id() -> u32 {
        cluster_client::meta::INVENTORY_PART_TABLE_ID
    }

    fn table_name() -> &'static str {
        "Part"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[
            sparse_struct!(ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                fields: &[
                    ProtobufKeyField {
                        path: &[Part::ID_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    }
                ],
            }),
        ]
    }
}

pub struct PackTable {}

impl ProtobufTableTag for PackTable {
    type Message = Pack;

    fn table_id() -> u32 {
        cluster_client::meta::INVENTORY_PACK_TABLE_ID
    }

    fn table_name() -> &'static str {
        "Pack"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[
            sparse_struct!(ProtobufTableKey {
                index_id: PRIMARY_KEY_ID,
                fields: &[
                    ProtobufKeyField {
                        path: &[Pack::ID_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    }
                ],
            }),
            sparse_struct!(ProtobufTableKey {
                index_id: 1,
                index_name: Some("ByPart"),
                fields: &[
                    ProtobufKeyField {
                        path: &[Pack::PART_ID_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                    ProtobufKeyField {
                        path: &[Pack::ID_FIELD_NUM_RAW],
                        direction: Direction::Ascending,
                        fixed_size: true,
                    },
                ],
            }),
        ]
    }
}