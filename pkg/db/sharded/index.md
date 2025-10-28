# Sharded DB

This consistents of multiple shards/tablets where each represents a single range of keys which is replicated via Raft to one or more servers. Each server may store one or more of these shards/tablets. To the user, all of these shards


For transactions, we assume that they are done 'interactively' (the full list of keys that are read/written is not known at the beginning of the transaction).

Note that we buffer all reads/writes on the client and only acquire locks when we are about to commit the transaction on the client side.

## Read Path

- On the first read, we contact the first shard for the requested key range and get a read timestamp 'T' based on the 'Now()' time of the server. The timestamp is an HLC and should be higher than any other time visible on the shard.
    - For this shard and all future ones we contact, we also keep track of the Raft log index. 
- On future reads, if they contact a not yet seen shard:
    - We first try reading the range at the highest timestamp available on the new shard
        - If the latest version of each value is '<= T', then all is good.
        - Else, we need to upgrade our read timestamp 'T'
            - Will need to check with all previously accessed ranges to ensure there are no conflicts.
            - If there are conflicts, we must restart the transaction, but restarting can simply use 'T' as a starting point

We should know the max allowed clock drift between servers 'E'. If the first time we attempt is 'T1', then the largest timestamp at which we will attempt to read at is 'T1 + E'

Note that if we encounter any pending transactions while reading, we will need to block for them to be commited since we don't know if the commit has completed yet when we started reading.

## Read-Write Path

First do reads and acquire a read timestamp 'T1' and a list of read indexes in the Raft logs on each shard.

- Then issue a 'Prepare()' RPC to all shards (both those for reads and writes).
    - These RPCs will return a timestamp value for that server
- After all return, we can return to the caller.
- Then issue a 'Commit()' RPC to all shards
    - This will contain the commit timestamp which will be the max of all timestamps returned by 'Prepare()'

- The client library will store the latest timestamp that it has observed. On future transactions, it will send the timestamp to the db servers that it contacts. This is to minimize the amount that the client needs to wait on future reads to the same key ranges.

TODO: Need to make things natively aware of transaction 'batch' (where we don't care about partial writes)

