// Wrapper around the RocksDB table properties object
// See:
// https://github.com/facebook/rocksdb/blob/master/include/rocksdb/table_properties.h
// The names of each property is defined here:
// https://github.com/facebook/rocksdb/blob/50e470791dafb3db017f055f79323aef9a607e43/table/table_properties.cc

use reflection::*;

/// TableProperties contains a bunch of read-only properties of its associated
/// table.
///
/// u64's are encoded as varint64's
///
/// TODO: When serializing, only serialize the ones that we have set? (at least
/// all those that don't have default values).
/// TODO: Most of these are more abstract than a single table
/// (i.e. num_deletions is also applicable when a table is used in the context
/// of an embedded database).
#[derive(Default, Reflect, Debug)]
pub struct TableProperties {
    /// the total size of all data blocks.
    #[tags(name = "rocksdb.data.size")]
    data_size: u64,

    /// the size of index block.
    #[tags(name = "rocksdb.index.size")]
    index_size: u64,

    /// Total number of index partitions if kTwoLevelIndexSearch is used
    #[tags(name = "rocksdb.index.partitions")]
    index_partitions: u64,

    /// Size of the top-level index if kTwoLevelIndexSearch is used
    #[tags(name = "rocksdb.top-level.index.size")]
    top_level_index_size: u64,

    /// Whether the index key is user key. Otherwise it includes 8 byte of
    /// sequence number added by internal key format.
    #[tags(name = "rocksdb.index.key.is.user.key")]
    index_key_is_user_key: u64,

    /// Whether delta encoding is used to encode the index values.
    #[tags(name = "rocksdb.index.value.is.delta.encoded")]
    index_value_is_delta_encoded: u64,

    /// the size of filter block.
    #[tags(name = "rocksdb.filter.size")]
    filter_size: u64,

    /// total raw key size
    #[tags(name = "rocksdb.raw.key.size")]
    raw_key_size: u64,

    /// total raw value size
    #[tags(name = "rocksdb.raw.value.size")]
    raw_value_size: u64,

    /// the number of blocks in this table
    #[tags(name = "rocksdb.num.data.blocks")]
    num_data_blocks: u64,

    /// the number of entries in this table
    #[tags(name = "rocksdb.num.entries")]
    num_entries: u64,

    /// the number of deletions in the table
    #[tags(name = "rocksdb.deleted.keys")]
    num_deletions: u64,

    /// the number of merge operands in the table
    #[tags(name = "rocksdb.merge.operands")]
    num_merge_operands: u64,

    /// the number of range deletions in this table
    #[tags(name = "rocksdb.num.range-deletions")]
    num_range_deletions: u64,

    /// format version, reserved for backward compatibility
    #[tags(name = "rocksdb.format.version")]
    format_version: u64,

    /// If 0, key is variable length. Otherwise number of bytes for each key.
    #[tags(name = "rocksdb.fixed.key.length")]
    fixed_key_len: u64,

    /// ID of column family for this SST file, corresponding to the CF
    /// identified by column_family_name.
    #[tags(name = "rocksdb.column.family.id")]
    column_family_id: u64,

    /// Timestamp of the latest key. 0 means unknown.
    #[tags(name = "rocksdb.creation.time")]
    creation_time: u64,

    /// Timestamp of the earliest key. 0 means unknown.
    #[tags(name = "rocksdb.oldest.key.time")]
    oldest_key_time: u64,

    /// Actual SST file creation time. 0 means unknown.
    #[tags(name = "rocksdb.file.creation.time")]
    file_creation_time: u64,

    /// Name of the column family with which this SST file is associated.
    /// If column family is unknown, `column_family_name` will be an empty
    /// string.
    #[tags(name = "rocksdb.column.family.name")]
    column_family_name: String,

    /// The name of the filter policy used in this table.
    /// If no filter policy is used, `filter_policy_name` will be an empty
    /// string.
    #[tags(name = "rocksdb.filter.policy")]
    filter_policy_name: String,

    /// The name of the comparator used in this table.
    #[tags(name = "rocksdb.comparator")]
    comparator_name: String,

    /// The name of the merge operator used in this table.
    /// If no merge operator is used, `merge_operator_name` will be "nullptr".
    #[tags(name = "rocksdb.merge.operator")]
    merge_operator_name: String,

    /// The name of the prefix extractor used in this table
    /// If no prefix extractor is used, `prefix_extractor_name` will be
    /// "nullptr".
    #[tags(name = "rocksdb.prefix.extractor.name")]
    prefix_extractor_name: String,

    /// The names of the property collectors factories used in this table
    /// separated by commas
    /// {collector_name[1]},{collector_name[2]},{collector_name[3]} ..
    #[tags(name = "rocksdb.property.collectors")]
    property_collectors_names: String,

    /// The compression algorithm used to compress the SST files.
    #[tags(name = "rocksdb.compression")]
    compression_name: String,

    /// Compression options used to compress the SST files.
    #[tags(name = "rocksdb.compression_options")]
    compression_options: String,
    /* TODO: THere are some more here (including thw whole_key_filtering:
     * https://github.com/facebook/rocksdb/blob/f059c7d9b96300091e07429a60f4ad55dac84859/table/block_based/block_based_table_builder.cc#L251 */

    /* TODO: user_collected_properties, readable_properties, properties_offsets */
}
