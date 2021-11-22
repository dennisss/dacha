


Objectives:
- Implement the core SSTable format used by LevelDB/RocksDB
- Implement support for LevelDB/RocksDB style embedded database

TODO: Implement Direct IO and fsync/fdatasync/fallocate were necessary.

TODO: Also abstract away the file i/o interface.

Comparison of RocksDB file formats:
https://github.com/facebook/rocksdb/wiki/A-Tutorial-of-RocksDB-SST-formats#block-based-table

This is how the table is generated (from compressed blocks):

- https://github.com/google/leveldb/blob/master/table/table_builder.cc
- Same for RocksDB: https://github.com/facebook/rocksdb/blob/master/table/block_based/block_based_table_builder.cc


Individual data entries described here:
https://github.com/facebook/rocksdb/blob/master/table/block_based/block_builder.cc#L21

TODO: Must go through all of the varints and parse them with the proper width (i.e. u32, u64, i32, i64, etc.)


TODOs:
- Write batch
- Pipelined log writing
- Deleting tables that are obsolete.
