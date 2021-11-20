# Metastore

This is a simple, small, and strongly consistent key value store for slow changing metadata. It can also serve as a distributed lock manager for othe serivces. This is meant to take the place of services like Google Chubby, Etcd, or Apache Zookeeper.

## Features

The store functions like a filesystem where each key should be absolute path (e.g. `/directory/file`). Keys should only contain valid file name/path characters. In the future we may support per-directory ACLs.

Supported features:

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


## Implementation

Implemented as a single Raft group using an LSM tree.

Each key in the storage has the form `[table_id] ...`.

Keys with `table_id=1` have the following format:

- `[table_id=1] [user_key] [sub_key=1]`
    - As a value, stored the value that the user has associated with `user_key`
- `[table_id=1] [user_key] [sub_key=2]`
    - Contains a `KeyLockInfo` protobuf describing how this 

TODO: How do we proxy values while ensuring that client ids are mapped correctly.

When a client connects, we will propose a no-op entry on the Raft log in order to get a unique `client_id` for it. 

## Transaction Lifecycle

Each transaction exists within the scope of a single raft leader's term. This means that all metadata associated with all ongoing transactions are stored in-memory on the leader node.

### Creating 

First we generate a unique id for the transaction of the form `[term]:[local_monotonic_id]`.

Then we acquire a linearizable read index from the raft::Server (this index should be at least as large as the final log entry known to the node at the time at which it become the leader).

We store the transaction (`id`, `read_index`) as a new `TransactionEntry` in the `TransactionManager` container along with all other ongoing transactions. At this point we also do some cleanup: if there are transaction entries in the `TransactionManager` from an earlier term, we delete them.

### Performing a read

Each read will acquire a temporary reader lock on the key range being read. Each `TransactionRangeLock` object contains a:

- `key_range`: Range to which it applies
- `ref_count`: Number of transactions reading from this range. Which this goes to 0 the lock is deleted.
- `write_lock`: If true, another transaction is currently writing to this range. Thus is can't be overwritten until that transaction is comitted. 

NOTE: All locked ranges are non-overlapping so if two transactions read overlapping ranges, each of them may end up having more than one lock to just read one range.

First we create or get the existing lock entry and increment its ref count. If the `recent_write_index` is set to a value greater than the current transaction's `read_index`, then we must stop the current transaction.

Then we look up the key from the head of the database. If the entry in the database has a higher sequence than your `read_index` we also cancel the transaction.

Else, we return the value to the client.

### Performing a write

This will simply append the 

NOTE: Writes are only linearizable if the key was previously read in the same transaction.



/*
    Typical transaction lifecycle (read modify write)
    - Step 1: Acquire a sequence pointer from the DB
        => Add to the Transaction
    - Step 2: Performing a read:
        - Increment the ref count on the key in the key_locks map and mark recent_write as None if not present.
        - Perform the read at the head of the DB
        - If the DB returned a value at a sequence higher than our transaction bail out.

    - Step 3: Application makes some change

    - Step 4: Performing the write:
        - Simply append the key to our write table

    - Step 5: Committing
        - Lock the metastore state
        - Atomically get the next commit index
        - Check that no read keys were modified and de-ref all of them.
        - Assuming we can proceed, loop back through the key locks and mark the dirty state of all of them
        - Finally commit using the read index acquired from raft at the start.

    Main



    So a transaction starts (marked by a beginning sequence in the db)
    - Read a key from the head of the DB.
        -


    Challenges:
    -

    When a transaction reads a key, it will

    An optimistic transaction would perform the write and bail out when being applied to the state machine if we noticed a c
*/


## Old


- Initial discovery of peers will be via multi-cast
    - Eventually rely on the list of tasks in the cluster task metadata as we want to ensure that all servers are accounted for.

- This will just expose a basic key/value store possibly with some read/write ACL support.

- Provide a client library 

- Must support the bootstrapping or joining existing via an RPC call
