# Embedded DB / SSTables

This library implements a single process key-value store based on the RocksDB / LevelDB format.

The main exports of this library are:

- `EmbeddedDB` : The complete LSM tree based key-value store by default using a WAL for writes.
- `SSTable` / `SSTableBuilder` : Readers/writers for the on-disk sorted key-value table format.
- `RecordReader` / `RecordWriter` : Append-only log reader/writer

