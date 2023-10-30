

Making a log entry database

- Use a key-value store
- Log entries of the form:
    - Key: `[Instance Id] [Zone] [Time Epoch] [Resource (Hash)] [Timestamp Delta]`
        - `[Timestamp Delta]` is the time since the start of the epoch
        - `[Resource]` is stored as a hash of the object and normally contains the `{ type: CLUSTER_JOB job_name: "..." }`
    - Value: Full data of what was logged.
    - Sorted in descending timestamp order to make it easier to query?

- How to query
    - Get the latest logs in a single cluster job
        - Scan backwards through epochs and match on Resource
    - Get the latest logs over everything.
        - Need to sort the 

- Indexed information (per time epoch)
    - `[Resource Hash]` to resource object map
    - Or ideally the DB would natively support this type of thing.
    - Posting lists
        - Keys: `[Resource Key Name] [Resource Key Value] [Resource Hash]`
        - Merge these to find all the resources we care.
        - Finally merge the resources to find the latest timestamps.

- Other special situations:
    - Error reporting
        - If we see an error, we want to raise it in some dashboard (maybe sorted by count)
            - Ideally extract a hash print of the error type and then can consider this a distributed 


File Storage
- If we store on 3 replicas each writing to 3 disks, then that's a lot of space usage.
- Database should have per-table replication options
    - Tables with the same replication settings can be stored together (but them later can't change replication without splitting them up)


What I need:

- Built out the generic database
    - Multi-Raft group
    - Different EmbeddedDB per Raft group
    - Don't 'need' distributed transactions for logging
    - For logging, splitting a key range should only need local consensus.
    - Basically have two tiers of key range metadata
        - Top level: `[Global] [Key Range] -> [Node Pool]`
        - Second level `[Node Pool] [Key Range] -> [Replicas]`

        - Alternative solution
            - Have a concept of 'instances'

        - Top level (Global key range table)
            - Single Raft group that everyone participates in
            - Maps key ranges to group ids
        - Group level
            - Map groups to replicas
            - May be configured to pin within some region

- Splitting a range
    - The master node assigned to each range monitors the size. If it becomes too large, it proposes a split point and timestamp
    - Then all nodes in the range split the table into two.
    - Once all are done, the split is marked as complete and old range is swapped out.
    - Then wait for any outstanding queries on the combined range to finish.

- Re-balancing
    - Global process (whoever is the leader)
    - Look the disk usage of each node and make roughly the same
        - Want to avoid proposing a change that just triggers sloshing on the next round
        - Any node can be probed to find the size of the ranges it has stored.

- Multi-tier storage
    - GCP Persistent Disk is expensive
        - Standard GCS: $0.02 per GB
        - Standard Persistent Disk: $0.04 per GB
    - After level 0 in the EmbeddedDB, place on regional GCS
        - But, this is already replicated to maybe 2 cells with maybe 3 disks per cell
        - So, don't need to have many replicas of this
        - Maybe easier to split at the DB level

Key features I need to do first:
- Streaming tar file generation and reading
    - Then we can stick it into the raft implementation
- Skip list based memtable

