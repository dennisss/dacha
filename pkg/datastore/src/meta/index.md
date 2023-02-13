# Metastore

This is a simple, small, and strongly consistent key value store for slow changing metadata. It can also serve as a distributed lock manager for other serivces. This is meant to take the place of services like Google Chubby, Etcd, or Apache Zookeeper.

## Features

Keys in the store are arbitrary user provided byte strings where prefixes can be used to group data into directories/tables.

Supported features:

- Key/value CRUD operations
  - Single key read/write/delete
  - Key range queries.
- Point-in-time reads
  - View the entire set of keys/values at a specific point in time.
- Transactions
- Advistory Locking

- Connection operations
    - `NewClient`: creates a new client connection
    - `KeepAlive`: periodically called to ensure that the client is still active.
- Key/value operations:
    - `Get(key) -> value`
    - `GetRange(start_key, end_key) -> { key, value }[]`
        - Used for listing files in a directory.
    - `Put(key, value)`
    - `Delete(key)`
    - Serializable transactions using any of the above operations.
- Lock service operations:
    - `Acquire(key, bool exclusive)`
    - `Release(key)`

Implementing locks:
- Have a row containing `{ client_id: X, last_seen: Y }`
- Create it using a transaction
- Also update it using a transaction
- Would be preferable to not store this data on disk
  - So the benefit of having a more formal API would be to avoid disk writes
- Minimally will write updates to the log.
- Watch events 

- If we failover to another master, it must also know 


## Implementation

The server is implement using a single Raft group which is used for consensus and writing to a key-value state machine based on LSM trees.

### Internal Tables

Each key provided by the user in an RPC request is not directly used as the key in the internal key-value storage on disk. Instead, the user key is mapped to internal keys which store the actual user data and associated metadata.

The internal key range is grouped into multiple tables. For example, the `UserData` table is used the user provided data on put/delete operations. Below are some examples of internal keys in this table:

- `[table_id=1] [user_key] [sub_key=1]`
    - Stores the value that the user has associated with `user_key`
- `[table_id=1] [user_key] [sub_key=2]`
    - Contains a `KeyLockInfo` protobuf describing any locks acquired by the user via the `LockService` API.

Another internal table used is the `TransactionTime` table. It only has a single key of the form `[table_id=3]` with its value being the timestamp of the most recent transaction performed on the store.

### State Machine

We use the `EmbeddedDB` interface to store the internal tables mentioned above on disk on each Raft replica:

- All mutations to the state machine are read from the Raft log as `WriteBatch` structs.
- Each `WriteBatch` has a sequence number set to it's Raft log index.
- Newly deleted/overriden values are kept in the database and are not compacted right away.
    - A special `CompactionWaterline` singleton table stores the value of the oldest sequence we want to compact.
    - Every hour, we search the `TransactionTime` table for the next oldest transaction from time 'Now - 24 hours' and set that transaction's sequence as the new value of the `CompactionWaterline` singleton.
    - Additionally we have custom logic to ensure that no individual key revision is compacted if the next revision after the compaction window was < 24 hours since now.

The complicated compaction scheme described above is used to ensure that long lived transactions and distributed snapshots are possible:

- We can't just use `EmbeddingDB::snapshot()` as that only gurantees snapshot consistency on a single replica.
  - Instead we need to use a 'read index/sequence' as our snapshot marker.
- We must preserve deletions for transactions as a Put followed by a Delete on a single key can hide the fact that the key has changed since the start index if we have already compacted away the deletion tombstone.
  - For example, if we start a read at t=0, write a value at time t=1 and delete it at t=2, the latest state of the database may not have any trace of the key so we can't safely commit the transaction
  - A simple solution to this problem is to 

Read-on;y possible

### Transactions

We support 'compare and set'-style transactions where we can:

- Verify that no keys in a set of key ranges read earlier have changed since some global read sequence.
    - To make this claim, we only accept requests with a read sequence >= the current `CompactionWaterline`.
- Atomically perform one or more Put/Delete operations.

All transactions are executed on the leader as follows:

- Acquire in-memory reader/writer/reader-writer locks to all ranges touched by the transaction
    - Conflicts will require overlapping transactions to block for the first one to finish.
- Verify that all read/compare conditions are still valid:
    - This can be done in parallel as each transaction holds a reader lock on the corresponding key ranges at this point which gurantee that no concurrent transactions in the same raft term commit first and override the values we are reading.
- Append the write operations to the Raft log
- Wait for the log entry to be applied to the local state machine
    - This must happen before we release our locks.
    - TODO: Given that the state machine uses MVCC, we should eventually implement parallel execution of multiple `WriteBatch`s on the `EmbeddedDB`.
- Release locks held for the current transaction.

TODO: If a transaction needs to be retried due to conflicts, have some mechanism to prioritize retrying of the longest waiting transactions first.

Transactions are executed as a single request to the metastore service so the aforementioned locks should be short lived as they don't block on client round trips.

TODO: Eventually further improve the liveness of the system by adding more restrictions to ensure that transactions are short lived such as limits on the size of each transaction.

Before the client issues a transaction, it may use other non-locking `KeyValueStore` service methods such as `Read()` and `Snapshot()` to cheaply prepare the contents of the transaction request.
