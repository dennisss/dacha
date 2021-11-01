# Metastore

This is a simple database with architecture similar in purpose to something like Chubby or Etcd, but doesn't intend on being a key-value store.

The database is composed of tables where each table's schema matches a protobuf definition.

## Architecture

- All data is stored in a single EmbeddedDB instance.



SSTable Key format:
- (Table ID, (Key, ...) Column ID)
    - We already have a schema in the form of protobuf definitions
    - Just need to define what the key will be.
    - Add special []

- Should we support nested protos.


proto_table!(
    id = 0,
    primary_key = [ name ],

);

MVCC
- Transaction
    - Represented as:
        - List of written keys with new values
            - Stored as a memtable
        - Read sequence
        - Transaction sequence
    - Reading from a transaction
        - Must first look into the written keys dict.
    - Commiting a transaction
        - Must acquire a lock for each written key (for now just use a Mutex<HashMap<key, txn_id>)
            - We could precompute the key hashes to improve performance.
        - Double check that none of the keys have changed since the last snapshot time.
            - Typically this will only require looking in recent memtables.
        - Then write to the memtable and to the log (single log entry)
        - Release locks
        - Advance the last_sequence (can be done by a log writer thread)
            - Once once the sequence for all prior log writes has already been
        - Return from transaction
            - Must only tell the user once the last sequence has been advanced.
            - Otherwise the user may read future 
        - Generally things are simpler if we control the log

- Also:
    - EmbeddedDB
        - Should be able to decouple the Log (and InternalKey) from the core compaction implementation

Name: TableStore
- EmbeddedDB will auto-assign a sequence to each transaction, but we need to verify no conflicting keys at a higher level