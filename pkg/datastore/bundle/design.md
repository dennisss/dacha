# Immutable File/Data Archive Format

This (`DataBundle`) is a format for storing a collection of files/data slices as a single packed collection.

- Many files are concatenated together so per-file metadata overheads are relatively low.
- Data is compressed
- Data is efficiently randomly seekable.

## Format

The archive is composed of multiple files which are all stored in one directory. Usage of multiple files rather than just one is to simplify the implementation of parallelized writing to the bundle 

- `/index/`
    - EmbeddedDB containing all the file metadata.
- `/shard-{%08d}`
    - Files containing a part of the data referenced by the metadata.

### Index

This is a standard EmbeddingDB table with the following key-value pairs

- `BUNDLE_CONFIG #` singleton key
    - Contains metadata on how the bundle was created
        - `block_size`
        - `record_size`
        - `compression_algorithm`
        - Magic string used to verify this is a bundle
- `FILE_METADATA # [directory]/ # [file_name]` keys (one per file)
    - Stores metadata about each file
        - `type`: `FILE|DIRECTORY`
        - `shard_index`: Which shard file contains this file's data
        - `shard_offset`: Start offset (in uncompressed data) of each shard at which this metadata starts.
        - `size`: Size in bytes of this file's data (for directories this is always 0).
        - `checksum`: CRC32C of the entire file. Mainly used for testingthe correctness of this implementation.
        - `user_data`: Custom `Any` proto provided by the creator of the bundle.
- `SHARD_METADATA # [shard_index] # [uncompressed_offset]`
    - Stores the byte offset in the shard file at which a compressed record starts which contains data starting at the given uncompressed offset. 
    - If data is stored uncompressed then these keys are omitted.
    - May have a bool called `zeros` which indicates that this entire record is formed of zeros so no compressed data was stored.

### Shard Files

These files are formed by:

- Starting with a magic identifier/metadata header
- Concatening many uncompressed files together.
- Every `record_size` bytes (256KiB) are compressed and appended to the file.
    - After each record, we pad up the disk's `block_size` (typically 4096)
    - Note that if we see the start of a large >1MiB file when accumulating record data, we will terminate the record and start compressing the large file at the start of its own first record.

At the end of this proccess, all the record offsets are stored in the index.

### FAQ

Can we re-use the Riegeli file format?

- Yes, but need separate metadata anyway.
- Would be slightly higher overhead due to all the stored headers which would be redundant.

Generalized storage:

- Record log
    - Each file is chunked 
    - If we know how many more bytes are allowed in the current chunk then we can just write that many chunks and go