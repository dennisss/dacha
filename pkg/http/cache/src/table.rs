use db_table::table::*;
use db_table::{table_id, sparse_struct};
use http_cache_proto::{RequestCacheEntry, RequestProto};

pub struct RequestCacheEntryTable {}

impl ProtobufTableTag for RequestCacheEntryTable {
    type Message = RequestCacheEntry;

    fn table_id() -> u32 {
        table_id!(31)
    }

    fn table_name() -> &'static str {
        "RequestCacheEntry"
    }

    fn indexed_keys() -> &'static [ProtobufTableKey] {
        &[sparse_struct!(ProtobufTableKey {
            index_id: PRIMARY_KEY_ID,
            fields: &[
                ProtobufKeyField {
                    path: &[
                        RequestCacheEntry::REQUEST_FIELD_NUM_RAW,
                        RequestProto::URL_FIELD_NUM_RAW,
                    ],
                    direction: Direction::Ascending,
                    fixed_size: false,
                },
                ProtobufKeyField {
                    path: &[RequestCacheEntry::TIMESTAMP_MILLIS_FIELD_NUM_RAW],
                    direction: Direction::Descending,
                    fixed_size: false,
                },
            ],
        })]
    }
}
