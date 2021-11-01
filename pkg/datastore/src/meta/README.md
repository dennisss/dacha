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

## Old


- Initial discovery of peers will be via multi-cast
    - Eventually rely on the list of tasks in the cluster task metadata as we want to ensure that all servers are accounted for.

- This will just expose a basic key/value store possibly with some read/write ACL support.

- Provide a client library 

- Must support the bootstrapping or joining existing via an RPC call
