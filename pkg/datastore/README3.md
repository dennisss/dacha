# Metastore

This is a simple, small, and strongly consistent key value store for slow changing metadata. It can also serve as a distributed lock manager for othe serivces. This is meant to take the place of services like Google Chubby, Etcd, or Apache Zookeeper.

## Architecture

Implemented as a single Raft group using an LSM tree 

## Supported Operations

- GetKey
- GetKey Range




- Initial discovery of peers will be via multi-cast
    - Eventually rely on the list of tasks in the cluster task metadata as we want to ensure that all servers are accounted for.

- This will just expose a basic key/value store possibly with some read/write ACL support.

- Provide a client library 

- Must support the bootstrapping or joining existing via an RPC call
