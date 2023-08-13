# Cluster Blob/File Storage

For everything from storing application binaries, logging/metrics, to user data (possibly in a higher level database), we need a robust way to store non-volatile data on disk.

This doc describes the storage layer we are using for directly interfacing with disks and organizing disk across machines to enable these usecases. This layer takes inspiration from ZFS, Google Colossus, HDFS, and similar distributed blob/file systems.

## Goals

The overall goal is to provide a single filesystem abstraction that scales to an arbitrary number of disks across an arbitrary number of machines with minimal performance overheads.

We want 99% of storage workloads to go through this system so that we can avoid individual applications having to deal with disk management.

Primary feature focuses include:

- Automatic data replication, integrity checking, and encryption of data.
- Append only workloads
- Eliminate single points of failure.
- No external dependencies
  - Solutions like HDFS/Tectonic assume there is ambient availability of a strongly consistent metadata service (Zookeeper/Chubby like). We make no such assumptions as our goal is to be able to run those types of metadata services 'on top' of the storage layer.
- Tunable read/write consistency model.

Non-goals:

- Compression
  - Clients must implement higher level compression of data if needed.
- Deduplication
  - De-duplication is challenging to reliably implement at a low level so should be done at a higher level if needed.
- Multi-network storage
  - Initially we'll assume that all the machines in one namespace are in a single LAN (communication between machines is fast and normally reliable). In other words, data replication across machines can be syncronous.

## Design

Here we explore how our storage system named `Blobstore` works.

### Overview

To a client, we expose a file API based on file URLs where a URL is of the form:

`blob://[storage_frontend_endpoint]/[directories]/[file_name]`

where `[storage_frontend_endpoint]` will usually be a zone-wide endpoint which defines a file namespace for a single cluster zone.

All files are accessible from any networked machine.

We will build this up as follows:

- Each physical disk contains a `Volume`.
  - A volume is a GPT partition with a custom filesystem.
  - The custom filesystem exposes an API for reading/writing `Chunk`s (id keyed large extents of disk space).
- `Disk` workers are assigned to each disk machine. Each worker:
  - Has all `Volume`s on the same machine are attached to is.
  - Implements the custom filesystem driver.
  - Periodically scans the `Volumes` to ensure that checksums are matching.
  - Exposes `Volume` reads/writes as an RPC service.
  - Implements a basic `Machine Pool` filesystem which allows using multiple disks together as a filesystem (can be accessed from the local machine without any external network dependencies).
- On top of `Machine Pool`s, we can build larger distributed databases:
  - `Metastore` for non-sharded HA cluster management.
  - `Datastore` for sharded general data storage (Spanner-like).
  - (these can't rely on the full `Blobstore` for cross machine replication so must implement their own mechanism)
- `Frontend` workers interface with the `Datastore` and use it for storing file metadata.
  - This is basically just a stateless API layer on top of the `Datastore`.
- `Janitor` workers asyncronously verify:
  - All `Volume`s contain all the intended chunks (and intended generation of the chunks) (may require replicating or deletion).
  - Heartbeat/ping `Disk` servers to determine if they are alive.
  - Increase replication of under-replicated chunks when disks/servers die.

When a client wants to read/write a file, they will first communicate with the `Frontend` workers to get information on the location of chunks and will then communicate directly with the `Disk` workers to transfer bytes. Precise file/chunk sizes are only stored on disk servers so most file writes only require communicating to the `Disk` servers (no database writes).

### Terminology

A quick reference to all the terminology that will be defined later:

- `Sector` (aka physical block size)
    - Smallest independently writeable region of the disk (constant for a given disk).
- `Block`
    - Smallest region of disk space which we keep track of (this is the smallest region for which we track the allocated/free status of the disk region).
    - MUST be a multiple of the sector size.
- `Record`
    - Contiguous region on disk that is protected by a single checksum.
    - Size MUST be a multiple of the `Block` size.
- `Segment`
    - Contiguous set of records that are encrypted with one MAC.
    - Size MUST be a multiple of the `Record` size.
- `Chunk`
    - Contiguous set of segments on a disk. A chunk may be replicated across multiple disks.
    - Represented by a unique id.
- `Stripe`
    - Set of chunks that are all used to replicate the same data.
    - There will mainly be >1 chunk in a stripe for reed solomon encodings.
- `File`
    - List of stripes where concatenating the data in each stripe represents all of the file's data.
- `Pool`
    - A set of disks to be considered for replicating a file.


### Chunk Representation

A chunk is a single big section of allocated space on a `Volume` for storing file data. As you'll see, it is the central abstract layer that exposes the `Disk` worker / `Volume` layer implementation to higher level management layers.

Chunks are:

- Typically fairly big (>= 1MB).
- Creation of new chunks is expensive but writing to chunks is cheap.
- Identified by a uint64 `chunk_id`.
- Support end append operations (optionally truncated to a prior offset in the chunk).
- Versioned using a uint64 `generation_number` number (used as MVCC).
  - All mutations to a chunk are atomic and transform one generation number into another higher generation number.

`Volumes` expose a key-value API for interacting with chunks:

- `List`
 - Lists all the `(chunk_id, list<generation_number>)` pairs that are readable on the volume.
- `Read(chunk_id, generation_number)`
  - Reads the data of a chunk at a specific frozen snapshot.
- `Append(chunk_id, last_generation_number, next_generation_number, data)`
  - Appends `data` to the chunk immediately after the data up to `last_generation_number`.
  - If the chunk already contains any data at a number >= `last_generation_number` then the operation will fail.
  - If there is any data at generations after `last_generation_number` at the start of this operation, it will be deleted before applying this operation.
  - After calling this `Read(chunk_id, new_generation_number)` will return `Read(chunk_id, last_generation_number) + data`.
  - After calling this operation, both `Read(chunk_id, new_generation_number)` and `Read(chunk_id, last_generation_number)` will still be sucessful
  - A special value of 0 for `last_generation_number` is used when performing the first write to a chunk. 
- `Delete(chunk_id, generation_number)`
  - Removes the data for a chunk at a given generation number.
  - Any data needed to store any other generation numbers than the one specified will be kept on the volume.

Normally a single `chunk_id` will be replicated across multiple `Volumes`. In this case, the `generation_numbers` will be used to determine which volumes contains the newest values. For this reason, it is also important to guarantee that `generation_numbers` are never repeated while in a degraded state (like some disks being offline or the server restarting).

We will create `generation_number`s in later sections of this design as follows:

- For chunks only replicated across the local server,
  - When the server starts up, it will wait for the system time to be initialized and then then decide on a value for 
    - `last_local_generation_number = max(unix_time_micros, latest_number_on_each_local_disk)`
  - Then each new generation number is chosen as `++last_local_generation_number`
- For chunks potentially replicated across many machines,
  - Numbers are pulled from the `Datastore`'s consensus log.

### Pools

A `Pool` is a named set of disks. Pools may be scoped to a single machine (a `Machine Pool`) or across multiple machines in the cluster. Pools may be used for:

- Identifying different storage types (flash vs HDD)
- Localizing resources (e.g. keeping all data on one machine)

Per `Blobstore` instance, we expose a single global namespace for file paths, but individual files/directories may be placed on one or more `Pool`s.

All `Machine Pool`s are defined in the metadata stored on that machine meaning that they don't require any external dependencies while cluster scoped pools are defined in the `Datastore`.

### File Representation

Every file exists in one or more `Pool`s (normally just one).

To open a file, you need to know the following:

- Path in the file system where to place it.
  - Paths may be relative to a single cluster zone or to a single machine
- Which disks the file can be stored on.
- Replication Parameters: Mirror count, whether or not to store parity data, etc.

Within a single pool, every file is a list of stripes which each correspond to a different extent of the file data. Each of these stripes is a set of one or more chunks.

Chunks are identified by their unique uint64 ids and are assigned to one or more volumes. Chunks in the same stripe should be on different disks (and different servers if possible). A single chunk id may appear on multiple disks though.

The protobuf representation fo a file is shown below:

```
file: {
  type: REGULAR|DIRECTORY
  replication_scheme: REED_SOLOMON
  num_data_replicas: 3,
  num_parity: 2,
  pools: [
    {
      pool_id: HDD
      stripes: [
        {
          # Defined later.
          state: OPEN
          read_generation: 1
          write_generation: 2

          chunks: [
            {
              id: 123,
              type: DATA|PARITY
              
              replicas: [
                {
                  worker: 'dfd44.disk.cluster.local',
                  volume: '1234-444656-9934',
                }
              ]
            },
            // ...
          ],
        }
      ]
    }
  ]
}
```

Directories are also represented as files but don't contain any data. The files in a directory can be discovered by 


### Disk Workers

This section describes internal details inside of the disk worker binary.

#### Server Startup

Server machines in our cluster will potentially be running jobs that touch private data or data which we want to ensure has been generated correctly and not tampered with. So we need some resistance to server/disk theft, basic tampering, and ideally a machine should support attestation that they are running trusted binaries only. At the same time, we don't want to physically enter passwords into each server to avoid needing manual effort to reboot servers in power outages. Without complex integration with hardware root of trusts like AMD PSP, we won't be able to make a perfect solution to these problems, so we'll define a good enough solution for a 'home' setup.

On server startup:

- Boot device
  - Loads Linux from standard unencrypted BTRFS partition
  - Contains basic executables like SSH and the 'Server Startup' binary.
  - Contains a 'pre-boot' certificate with the private key stored in a local TPM
  - Defers control to the 'Server Startup' binary
- Server Startup binary
  - Queries a remote 'Key Server' to ask for a decryption key
    - Each decryption key is unique for each server
    - The query is signed using the 'pre-boot' client TLS certificate
    - The key server verifies that the ip is in an accepted set
    - The key server requests that a user approve the request on a phone app.
      - The user should be able to approve all requests from an ip for the next 30 minutes
  - If we don't get a decryption key immediately, we will register ourselves with the remote key server and wait for an operation to manually approve startup.
  - The decryption key is used to decrypt an encrypted BTRFS partition on the boot device which contains:
    - 'Machine Certificate': Used to authenticate all cluster node requests
    - 'Cluster Root Certificate': Trusted root certificates that are used to sign cluster machine/worker certificates.
      - (Public key only. Used as our root of trust)
    - 'Disk encryption key': One of the keys that can decrypt encrypted partions on all the disks attached to this machine
    - All binaries and metadata for a local cluster node to run
      - Binaries are cached copies of what is on the storage drives
- Cluster Node Runtime starts running
  - Starts all workers (one of which will be a 'Disk Worker' which is granted ownership of all disks)
  - Other workers may also start up on the same machine which may need to wait for 'Storage' worker to come online.
  - See the cluster encryption design doc for more information on how encryption keys are managed here.
- Disk Worker
  - Opens all the bulk storage (non-boot) disks attached to the machine.
    - All reads/writes to these disks go through this worker.
  - Exports access to the disks via RPCs.

TODOs
- Ensure that all keys needed to unlock all data are backed up in the cloud


#### Volume Format

Every disk used for cluster storage (aside from machine boot drives) uses a GPT partition table with a single partition with a custom filesystem type. We will call one copy of this filesystem a `Volume`. The filesystem is structured as:

- We use the partition GUID to identify this disk as a 'volume' in our cluster.
- The first sector (4K) is immutable and contains fixed information:
  - Size of the following log partition
  - Total size of the disk.
  - Block size to use on this disk (can never be changed)
  - Major version/generation of the file system.
  - Flags about encryption
    - TODO: Must have a way to verify we have the right key.
  - Checksum of this sector.
  - Configuration for redirecting the log for this disk to another pool.
    - TOOD: Encrypt this?
- Then a 128MiB 'partition' which is a cyclic log.
  - TODO: Place this at the end of the disk. This way it's unlikely the header is damaged in a write and it is easy to write to the log immediately after a write to a lower offset sector (HDD seeks go in one direction)
  - Each entry is 4KB (1 physical sector)
  - Each entry has the fields:
    - CRC
    - Sequence Number
    - Num internal entries
    - Length
    - Repeated list of `VolumeFile` entries.
      - See the next section for anexplanation of what these doe.
- The rest of the disk is a set of blocks:
  - Every block is a fixed size (4KB) and has the following format:
    - 'Checksum': 4 byte CRC32C
    - 'Data': Rest of the data
  - Multiple blocks are grouped into a fixed size segment (16KB). the data portion of these blocks contains:
    - 'IV': 16 bytes
    - Ciphertext
    - 'MAC': 16 bytes
    - Empty space is filled explicitly with zeros
  - Multiple segments are combined into a chunk of size 1MB
    - A 1MB span of sequential bytes are reserved right before the first byte of a chunk is written.
    - Chunk metadata are stored in the disk level metadata. Types of metadata include:
      - Extent
      - Size
      - u64 Chunk Id (or 0 if unoccupied)
      - Generation/version number
      - Which encryption is being used if any.
  - Appending to a chunk with the last segment completely written is cheap.
    - Simply need to write to the blocks immediately after the last segment
    - New generation can be described by aat least one extent still. 
  - Appending to a partially complete segment is more expensive
    - Need to re-encrypt the last partial segment
    - New copy of the segment is appended after the old version of the segment
    - New generation can be described by at least two extents.

Notes on encryption:

- The log and user data should always be encrypted.
- Depending on the file mode, the encryption 'segments' may be opaque to be volume as the encryption may be performed either on the server side (if its ok to use cluster level keys) or on the client side (if using per-client keys).
- Block checksums are not encrypted so may be manipulated by a bad actor (possibly silently replicating bad data to other disks).
  - As they are placed inline in the blocks, they can't be encrypted along with the block data itself as the disk servers may not be able to decrypt client-side encrypted blocks.
  - As an alternative we could have put the checksums in the encrypted volume log, but this would amplify the safe of the log.
  - TODO: Instead include in the chunk metadata table a rolling checksum of the whole chunk which we can verify.

Implementation notes:

- For a data block to ever to ever be re-used across different chunks, the driver must first flush a log entry declaring the block as deleted before starting the new write
- TODO: Disks may silently re-order writes:
  - https://openzfs.github.io/openzfs-docs/Performance%20and%20Tuning/Hardware.html#command-queuing
- TODO: If we delete a file and add a new copy of it, we may not TRIM all needed blocks in the case of a power failure
  - So after we detect a power failure, need to re-TRIM all free blocks at least once.


Dealing with corruption:

- The log segment is initialized to a set of valid No-op entries. If the log tail has corrupt entries, the disk will be marked in recovery mode.
  - For example, suppose a chunk is truncated and blocks are re-used for a different chunk, if the log entries for these entries are corrupted then we don't know what the correct state of all blocks are. 
- To mark a disk as good, we must verify by checking other disks with replicas of the same chunks that this disk contains the latest value of each chunk.


#### Local Volume Files

There is an internal file namespace (separate from the global `Blobstore` file path namespace) which is local to each `Volume` that is used to store metadata about that `Volume`.

The `Volume`'s log is a list of files where each file is a list of block extents representing on the data on disk. The entire set of files can be discovered by reading through the entire log with newer log entries overwriting earlier log entries referencing the same file. Eventually the log will wrap around and we will snapshot the list of files into a file.

This volume-level filesystem is mainly for internal usage as it is not replicated. The files stored in this filesystem are:

- `/snapshot`
    - Set of all local files that may have been truncated from the log (used as a starting point when replaying the log).
- `/config`
    - VolumeConfig proto: contains a list of all `Machine Pool`s which this volume participates in.
    - Also contains the list of all local file namespaces that are defined. 
    - TODO: In server startup, we need to reconcile this file across different volumes (make sure they all have a consistent view of volumes are in a pool).
- `/chunks/...`
    - EmbeddedDB containing mapping from chunk ids to the block extents they represent on this disk.
- `/namespaces/[name]/...`
    - EmbeddedDB containing file metadata which is replicated on a single `Machine Pool`.
      - This is a map from file path to file proto (containing chunk ids).
    - Note that is strictly for defining a new file namespace. Files within the namespace may have their data stored on any local pool.

The namespace table directories (`/namespaces/[name]/...`) will be automatically mirrored across all the disks in a single pool:

- The directories are treated as if they had the following replication settings
  - Plain mirroring across all N disks in the pool.
  - Quorem writes.
  - Quorem reads. 
- When a write operation arrives in the driver to a pool file,
  - It is assigned blocks on each disk are is written to disk potentially concurrently with other writes.
  - Once a quorum of disks have written the data, the operation is assigned a sequence number and appended based on that number to the partition's log (the log entry contains this sequence number).
    - If multiple operations in the same log batch touch the same pool, only the last one's sequence number is stored.
- On restarts of the server,
  - The driver reconciles any diffs between the directories
    - On each volume, we find the latest pool sequence number.
    - If at least a quorem of volumes aren't present, we don't allow opening the volume.
    - The highest seen sequence number is picked as the definitive value.
    - On any disks that aren't at this sequence number, we diff the files and make whatever changes are needed to make them in sync.
      - Finally these disks are upgraded to the latest pool sequence number.
    - The driver finishes startup and starts allowing requests 
  - One `EmbeddedDB` instance is opened by the disk server.

When the disk server starts up, it can determine the complete set of allocated blocks on each disk by loading into memory the `LocalFile` entries from the log in addition to the `/chunks/` table.

Metadata redirection:

- To speed up operations, the volume filesystem can be redirected to any other location/path (e.g. placing it on a separate SSD pool).
- When this happens, the log portion of the volume be disabled and the 128MiB section can be re-used for storing regular data.

TODO: How do we resource manage how much disk space is allowed to be used locally vs how much can be used remotely in the global layer. If we are really concerned then we can separate out which volumes are used for global vs local situations.

TODO: Must mark which chunks are used locally (so shouldn't be controllable by external control plans).

#### Disk Worker Namespace

For the purposes of being able to cross reference other namespaces in the same disk worker, each disk worker also exposes a virtual blob namespace which allows referencing the internal namespaces:

- `blob://localhost/volumes/[volume-uuid]/`
- `blob://localhost/namespaces/[namespace-name]/`

Note that each disk worker will acquire a Cluster Node unique named TCP port so `localhost` without an explicit port specified is guaranteed to point to a well defined disk worker instance.

#### TODOs

The disk worker also needs to handle:

- IOPS balancing/throttling
- Basic authentication to ensure the client is allowed to read/write the chunk.
- Providing an IPC/Shared memory interface for low overhead reads/writes from the same machine.

### Cluster-wide Namespace 

By using machine pools/namespaces, we can:

1. Deploy a `Metastore` instance
2. Start running the full cluster control plane.
3. Deploy a `Datastore` instance (also using machine pools for storage).

At this point we create a table in the `Datastore` which maintains the cluster-wide mapping of file paths to chunk/stripe metadata. Clients will interact with this table indirectly via Storage `Frontend` workers which handle per-user authentication.

#### Life of a write

To perform a write to a file, a client:

- Contacts a frontend to initiate the operation.
  - Frontends support the following write operations:
    - `CreateFile()`, `StartAppendToFile()`, `CommitAppendToFile()` `DeleteFile()`, `SnapshotFile()`, `RenameFile()`
  - The most interesting one is `StartAppendToFile()` which:
    - Finds or creates an empty stripe at the end of an existing file.
    - Locks that stripe so that the client has exclusive access to it (lock held in the `Datastore`).
    - Returns a `read_generation`, `write_generation` and a list of volume/server replicas.
      - The client will lease out some number (usually 128) of sequential `write_generation` numbers that can be used. Any unused ones are returned at the end. 
  - Note that other operations such as modifying an existing byte range must be represented using the above operations
    - Normally one would create a temporary file with a snapshot and then 
- Writes the data to each disk server containing the chunk.
  - Assuming mirrored replication and we want resilience to N failures, the client must wait for at least N+1 writes to finish before proceeding.
  - If a replica fails, the client can ask a frontend server for a new one (assuming the client can't meet replication requirements with the healthy replicas).
- Contacts a frontend again to finalize the write.
  - Informs the frontend/datastore of which writes were successful.
  - The frontend atomically verifies that the old lease is still valid (no other writers started) and updates the `read_generation` of the chunk to the new `write_generation` that was used for writing.
- Periodically heartbeats the frontend to maintain the lock and notify it whether or not additional writes have occured.
  - This heartbeat can also be used to acquire more 
- A client can continue to append to a chunk while it believes it still holds the lock and has a free `generation_number`.
- Once the client is done all writes, it will tell the frontend to relinquish its lock.

Note that creating a new chunk always requires contacting a frontend. When a new chunk is needed, the chunk is assigned a max size which is checked by the frontend against the user's quota before authorizing the chunk to be used (this way individual disk workers don't need to deal with quota management).

TODO: Need some anti-churn policy. Limit the max number of times that we offer replacement disks when attempting to perform a write a chunk.

#### Life of a read

Client:

- Contacts a frontend which tells it:
  - Which chunk contains the desired byte range
  - Which generation number to read.
  - A replica which contains the given generation
- Client contacts the disk server replica directly and reads it.
  - Note that concurrent writes to a chunk may mean that the requested generation has been replaced by a newer generation.
  - We strictly allow appending to chunks, so it should be ok for the client to receive data from a higher generation. 

Note that disk workers 'try' not to have any in memory cache so most reads are expected to trigger a disk read.

#### Stripe State

Every stripe effectively has the following information associated with it:

- `read_generation`
  - Last generation number known to be durably replicated. We will never allow reading generations less than this.
- `write_generation`
  - Next generation id that can be written. Expected to be higher any attempted 
- `state`
  - `OPEN`
    - A client is holding a lease with the chunk manager job to perform writes.
    - This client is currently writing a version of the chunk at `write_generation` or higher to disk servers.
    - At least `read_generation` is known to be fully written.
    - Eventually if the client times out or gracefully ends their lease, the state will transition to `CLOSED`
    - If this state is exited due to a timeout, then the manager must check with all the chunk replicas to determine what the highest new `read_generation` and `write_generation` is.
    - In this state, strongly consistent reads by a client other the lease holder must contact a frontend and wait for at least one heartbeat to come from the least holder
  - `CLOSED`
    - No client is currently writing to this chunk.
    - Normally this state will be entered by a writer gracefully ending their lease and providing the chunk manager job with a new value of `read_generation` which has is known to be durably replicated.
  - `FINALIZED`
    - All writes to this stripe are complete. No further writes are allowed and the stripe can only be deleted if it isn't needed.
    - This is normally once a client has written the max size to one stripe and rotates to a new chunk.
    - This state is used to support snapshotting of files
      - Multiple files can reference the exact same chunk so long as those chunks are FROZEN (mutations require making a new chunk).

TODO: If a client is the only writer to a log and reads it on every restart, it is undesirable for the log to suddenly get bigger after the startup read but before the client appends more writes due to new replicas coming online.

Normal user journey:
- Acquire lock at file open time
  - Write things to the files
  - Asyncronously 
  - Once enough writes are done or we ack the write to a frontend we can 

#### Generating Chunk Ids

Chunk ids are randomly generated uint64 ids. Chunks ids that are allocated by a disk worker for usage on a local pool will have the top 4 MSBs masked set to `1111`. Global ids may have these bits set to any other value.

TODO: Write global ids to an index in the Datastore to prevent collisions.

### Consistency Model

Depending on reader/writer requirements, we want to provide guarantees around the following properties:

1. A write must continue to be visible even after `K` random disks fail.
2. For strongly consistent writes, if a write is lost, then the loss is detectable (won't revert to an older version of a chunk).
3. The last write to a file wins
  - This is guaranteed by our usage of monotonic generations that should never re-use ids even if a disk with a high generation number is removed from the system and re-added later.

Additionally we have safety concerns:

- If a stripe is under replicated, we should avoid writing to it (or in general deprioritize individual volumes for receiving writes if they are critical to storing under replicated chunks).

For a strongly consistent write, suppose we want to mirror it across N replicas:

- To satisfy #1, we must successfully write to at least `K + 1` replicas
- To satisfy #2, we must either:
  - Successfully write to at a quorem of `(N / 2) + 1` replicas
    - (must be checked by readers if the stripe state is in the `OPEN` state)
  - Write to at least one replica and write the new generation to the file metadata.
    - Note: To avoid violating #1, we should only do this if all the disks required for #1 are written.

So we must wait for either of the above to finish until we return success to the user.

For Reed Solomon encoding with `N` word block and `M` code blocks,

- To satisfy #1, we must successfully write to at least `N + K` replicas.
- To satisfy #2, we must either:
  - Sucessfully write to a quorem `((N + K) / 2) + 1`
  - Write the new generation to the file metadata.

So in the write path, a client can specify (aside from the encoding type):

- `K`: number of disks we must tolerate failing
- `W`: raw minimum number of disks to write a stripe must be written before we return success (only makes sense for mirrored replication)
- `S`: bool whether or not we should require strong consistency (this may override `W` is it is not high enough)
  - If this isn't true, then we will lazily try to perform the metadata write to datastore.

In the read path, we can specify the following (only relevant if the stripe is in an `OPEN` state`):

- `R`: minimum number of disks which must be contacted to find the highest current generation number
- `S`: bool whether or not we should require strong consistency
  - If true, then `R` is increased at least high enough to query a quorem of disks.

Note that if the file metadata has stored a `read_generation`, we will never read a generation less than that. 

### Chunk Assignment

Assuming we know that we need to add a new stripe to a file with N chunks, how do we:

1. distribute the chunks to N disks and then 
2. place the chunk optimally within the disks.

For problem #1, we do not want to enforce that N is equal to the total number of disks in the cluster. Instead we first restrict the decision to a subset of subsets using the copy-set algorithm. Only copy sets spanning the max number of servers are used (to prefer spreading across as many failure domains as possible).

TODO: Also avoid assigning to disks that are at high occupancy already.

For the first stripe of a file, we simply select a random copy-set.

For future stripes, if the file is expected to be very large, we may choose to alternative between different copy-sets for different stripes of stripes to improve distribution of the file onto more disks. But, for small files, we will try to simply continue using the same disk set until space becomes an issue.

For problem #2, we want to store chunks that correspond to the same file or to files in the directory near each other (and sequentially for single files) to make them quick and easy to read.

We will subdivide a single disk into K 'cylinder groups' where it is relatively cheap to seek within a single group. A reasonable value for K would be 16.

For the first chunk associated with a file, we will:

- Hash the directory name to map to one of the K groups
  - We will also hash the file name to 256 bins. If a single bin in a group consumes more than 40% of the capacity of the group, we will assign future chunks to the next group (with + 1 index).
- Randomly select a location within that group to place the chunk.
  - If the group is full, we will square the bin number (mod k) and try again.
    - TODO: Pick a better probing sequence that hits all groups once.

For sequential chunks, the client will provide similar hash information but also provide a 'last chunk id' hint which is the last chunk on the same disk for the file. The disk server will try to place the chunk immediately after the last one in the file.

Additionally, a client may reserve/preallocate many chunks for a file and the disk server will mark them all as reserved for a given sequence chain.

TODO: Determine if we need to space out sectors of a single file (e.g. can we quickly read/write multiple sequential sectors of a disk without having to wait for another rotation)

