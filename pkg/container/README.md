# Cluster Runtime / Orchestration framework

This is a system for managing a fleet of machines and assigning work to run on them. This is similar to other systems like Google's Borg or Kubernetes.

## Terminology

- `Container`: Set of processes running inside of an isolated environment (using Linux cgroups,
  namespaces, chroot, etc.).
    - Individual `Container` instances will usually be identified by ramdom uuids and will be
      treated as ephemeral: if a `Worker` ever crashes and needs to be restarted, it will be assigned
      a fresh new container.

- `Node`: A single machine in a `Cluster` which has a fixed resource ceiling for running `Workers`
  locally.

- `Zone`: An isolated collection of `Node`s. One should typically have one or more `Zone`s per data center or geographic region. All machines in a zone are expected to be well connected in the network and each zone should be completely self sufficient in terms of workload management capabilities.

- `Worker`: A set of `Container`s running in a shared resource envelope on a single `Node`. Usually
  this will only be running a single `Container`.

- `Job`: A replicated set of `Worker`s with the same configuration.

- `Manager`: Special process which manages the state of the cluster.
    - There will be a single `Manager` `Job` per `Zone` with one leader `Worker` at a time which ensures that the cluster is in a healthy state.
    - This process also hosts the user facing API for performing CRUD operations on `Job`s.

- `Metastore`: Strongly consistent and durable key-value store and lock service used to store
  the state of a `Zone`. There will be exactly one of these per `Zone`.

- `Blob`: A single usually large binary file identified by a hash. Blobs may also have a small amount of metadata such as a content type (e.g. tar or zip) to describe how they should be processed.

- `Bundle`: Collection of files typically containing a binary + static assets and distributed as one or more `Blob` archives.

- `Volume`: Mounted path in a `Container`. Typically the source will be a `Blob` or a persistent directory on the `Node`.

- `Attempt`: A single try at running a `Worker`. Typically this makes to one or more `Container`. Each `Attempt` is identified by a the start time of the first container in the `Attempt`.

## User Guide

This section describes the main user journeys for creating a cluster, updating it, and using it to run user workloads.

Note: Currently we assume that you are executing all `cluster` binary commands mentioned below in the same LAN as your cluster.

Note: Currently only one cluster can happily exist in each LAN network.

### Node Setup

The first step in setting up a cluster is starting at least one node machine to run the `cluster_node` binary. If you later want to add nodes to an existing cluster, this process is identical.

We will present two sets of instructions:

1. For a 'Generic' machine: If you want to setup a node on your machine / Linux flavor of choice.
2. For a 'Raspberry Pi' : Simplified instructions if you are going to be running on a Raspberry Pi.

#### Generic

##### Prerequisites

**Linux packages:**

- Pre-compiled `newcgroup` binary.
  - Run `./pkg/container/build_newcgroup.sh` to compile it into `./bin/newcgroup`
  - `build_newcgroup.sh` MUST be run as the user that will run the node.
- `sudo apt install uidmap`
  - Provides the `newuidmap` and `newgidmap` SETUID binaries for enabling us to support using a range of
    user ids for running containers while running the runtime binary as an unprivileged user.
  - To user running the node binary MUST have a large set of uids/gids mapped in `/etc/subuid` and`/etc/subgid`.

**Configuration**

We require that cgroups v2 are enabled for all subsystems on the machine running the node:

- Verifying whether cgroups v2 is setup correctly:
  - Running `cat /proc/cgroups` should show a hierarchy (second column) value of 0 for all rows.
  - Running `cat /proc/mounts | grep cgroup` should show a `cgroup2` mentioned at `/sys/fs/cgroup` (not in a 'unified' subdirectory).
- If this is not the case, then systemd must be reconfigured as follows:
  - Verify running at least version 240 of systemd (check using `apt list | grep systemd`)
  - Add `systemd.unified_cgroup_hierarchy=1 cgroup_no_v1=all` to the systemd / linux arguments
    - In Ubuntu this is done by appending these to `GRUB_CMDLINE_LINUX_DEFAULT` in `/etc/default/grub`
      and running `sudo update-grub`.

If running in production on a dedicated node machine, we recommend the following config steps:

- Create a new user to run the node runtime
  - `sudo adduser --system --no-create-home --disabled-password --group cluster-node`
  - Append to both `/etc/subuid` and `/etc/subgid` the line:
    - `cluster-node:400000:65536`
    - The above line assumes that no other users are already using this uid/gid range.
- Create the node data directory
  - `sudo mkdir -p /opt/dacha/data`
  - `sudo chown -R cluster-node:cluster-node /opt/dacha`
  - `sudo chmod 700 /opt/dacha/data`
- Use `/opt/dacha/bundle` as the root directory when storing repository assets like built binaries
  - e.g. The `newcgroup` binary should go into `/opt/dacha/bundle/pkg/container/newcgroup`

If just doing local testing/development as your user, you only need to create the `/opt/dacha/data` directory and make it owned by `$USER` on your local machine.

##### Running

It is up to the user to figure out how to ensure that the node binary is run at startup on the node machine as the `cluster-node` user. But, we recommend using the systemd service located at `./pkg/container/config/node.service`.

To run a node locally for testing (accessible at `127.0.0.1:10400`), run the following command:

```
cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto
```

#### Raspberry Pi

In this section, we will describe the complete canonical process for setting up a Raspberry Pi as a cluster node.

Note: Only using a 32-bit Pi OS is supported right now.

**Custom Image Features**

We will be using a custom built Raspbian Lite image which has the following major deviations from the standard distribution:

- Packages/users needed for running a cluster node are pre-installed.
- Has UDev rules to allowlist all GPIO/I2C/SPI/USB/video devices for use by the the `cluster-node` user.
- Disables unneeded features like HDMI output / Audio.
- On boot, disables WiFi if Ethernet is available.

**Step 1**: Create an ssh key that will be used to access all node machines.

- `ssh-keygen -t ed25519` and save to `~/.ssh/id_cluster`

**Step 2**: Configure the node image.

Create a file at `third_party/pi-gen/config` with a config of the following form (by sure to populate the marked fields):

```
IMG_NAME='Daspbian'
ENABLE_SSH=1
PUBKEY_SSH_FIRST_USER='<PASTE FROM ~/.ssh/id_cluster.pub>'
PUBKEY_ONLY_SSH=1
TARGET_HOSTNAME=cluster-node
DEPLOY_ZIP=0
TIMEZONE_DEFAULT=America/Los_Angeles
LOCALE_DEFAULT=en_US.UTF-8
WPA_ESSID=<NETWORK_NAME>
WPA_PASSWORD=<PASSWORD>
WPA_COUNTRY=US
``` 

Best practices:
- It is recommended to configure separate WiFi and Ethernet images where the Ethernet image would have the `WPA_*` variables above commented out.
  - This ensures that wired Pis have exactly one ip and no traffic accidentally goes other wifi.

**Step 3**: Build the image:

Run the following commands to generate the Raspberry Pi SD Card image using the aforementioned config:

```
cd third_party/pi-gen
./build-docker.sh
```

**Step 4**: Flash the image located in `third_party/pi-gen/deploy/YYYY-MM-DD-Daspbian-lite.img` to all Pi SDCards.

**Step 5**: Power on a Raspberry Pi with the above SDCard

**Step 6**: Initialize the Pi:

Run the following

```
cross build --target=armv7-unknown-linux-gnueabihf --bin cluster_node --release
cargo run --bin cluster_node_setup -- --addr=[RASPBERRY_PI_IP_ADDRESS]
```

This will copy over the `cluster_node` binary, setup the persistent service and more.

You are now done setting up your Pi!

### Cluster Initialization

Now that you have at least one node running the `cluster_node` binary, we want to not initialize the cluster by starting up the control plane (metastore and manager jobs) on the cluster. To do this you need to know the ip address of one node in the cluster. For example to initialize a node running locally run:

```
cargo run --bin cluster -- bootstrap --node_addr=http://127.0.0.1:10400
```

The above command should only be run ONCE on a single node in your LAN. All other running nodes in the same LAN should automatically discover the initialized control plane and register with it via multi-cast.

Note: Currently only nodes on the same LAN can be added to the same cluster.

### Running a user workload

TODO: Describe how to bundle files/binaries together.

TODO: How to create a JobSpec file and start it (or update it) on the control plane.

TODO: What environment variables are present to workers.

Each dot separated component of the job name must match `/[A-z0-9_\-]{1,63}/`

For a job that requests N replicas, N workers will be created by the manager.  Each worker will be
named `[job_name].[worker_id]` where `[worker_id]` is a DNS name safe id. Note that we
intentionally do not support fixed ids like '0', '1', '2' (like in Kubernetes replica sets) as this
increases the complexity of other components of the system. Instead our ids are generated only with
the following guarantees:

- Ids are never re-used for new workers
- A worker instance/id will only ever be assigned to a single node.
- If a worker is updated in a way that doesn't require moving it to another node (e.g. in-place
  flag change), the node it is currently assigned to will restart it using the same id.


### Networking

Unlike some systems like Kubernetes which define one IP address per pod / worker, each worker in our system shared the IP address of the node. To enable communication between jobs, the cluster runtime manages the assignment of ports and provides a Rust client library for discovering other workers.

First a server will declare that it has network ports in its JobSpec as follows:

```
name: "adder_server"
replicas: 1
worker {
    ports {
        name: "rpc"
    }
    # ..
}
```

Then at runtime the server can find the port number by parsing a `rpc_util::NamedPortArg` argument.

Later when a client wants to communicate with the server, it should use the `cluster::ServiceResolver` when establishing a connection to a specific cluster DNS name.

Every distinct entity in the cluster is assigned a virtual DNS name of one of the following forms:

- `[node_id].node.[zone].cluster.internal`
- `[worker_id].[job_name].worker.[zone].cluster.internal`
- `[job_name].job.[zone].cluster.internal`

If the 

TODO: Document how name resolution internally works.

### Cluster monitoring

TODO: How to view registered nodes, running jobs/workers, etc.

## Design

### Node

#### Functionality

Each node is effectively just a service for running individual workers.

The node runtime itself should be able to operate independently with all external dependencies missing. This is required as we will typically run those external dependencies as workers on nodes.

But, the `Node`s don't do any cluster management. Instead assignment of workers to nodes is managed by the `Manager` jobs. The main outgoing RPCs from the node runtime are:
- `Metastore`
  - **Self Registration**: When running in a LAN, on startup each Node discovers the metastore via multi-cast and registers itself in the NodeMetadata (updates the ip address and port at which it is reachable).
    - This is mainly needed to support potential re-assignment of IPs in LANs over time if nodes restart.
    - This could be replaced by requiring static ips for nodes or in the cloud this could be replaced with a lookup by the manager in the VM API (or using static hostnames with lookups over regular DNS).
  - **Worker Reconciliation**: Each Node will watch the WorkerMetadata table to see if any workers are assigned/un-assigned to the node and then apply these changes localyl.
  - **Health Checking**: When a worker running on the node transitions to being healthy/unhealthy, the node updates the Metastore so that clients of the worker know that they can/can't send requests to it.
- When a node is missing a blob, it may look up a known replica list from the metastore and fetch it from another node.

#### Local Storage

Each node has a local EmbeddedDB instance it uses to store information such as:
- Which workers are running on it.
  - So that they can be automatically restarted (if enabled) on node reboot
- Record of state transitions for current/past workers.
- Stdout/stderr output of containers.

#### Node Permissions

Each node in a cluster runs the `cluster_node` binary which implements the container runtime.
This binary runs as a non-root unprivileged user starting with no capabilities (the `cluster-node` user in production clusters).

- The main node process will get capabilities by cloning into new Linux namespaces.
- Data created by the node is stored in `/opt/dacha/data` and should be chmoded with '700' so by default can't be accessed by containers unless we explicitly grant them owners access to specific subdirectories.
- Other data on the system is not managed by the node, but the general recommendation is to ensure different resources are owned by different groups. These can later by granted as supplementary group ids to containers.

When the binary starts, it will:

1. Run `clone()` with
  - `CLONE_NEWUSER`: Will be used to own the other namespaces created by this `clone()` call and grant permission to our unprivileged user to run as other users (see steps 2).
  - `CLONE_NEWPID`: Creates a new pid space for all containers. The main reason for this is to ensure that when the container_node binary dies, all containers it was running also die instead of ending up detached and unaccounted.
  - `CLONE_NEWNS`: New mount namespace so that we can remount `/proc` in the child.
1. The parent process will call the `newuidmap` and `newgidmap` binaries to setup the `/proc/[pid]/uid_map` and `/proc/[pid]/gid_map` maps for the child process.
  - We will use an exact mapping from ids in the child user namespace to ids in the parent namespace for all ids in `/etc/subuid` and `/etc/subgid` for the current user in addition to the current user id itself.
  - Note: This must be completed before the child does anything pid related. 
1. The parent process will call `newcgroup` to create a new cgroup v2 hierarchy at `/sys/fs/cgroup/dacha`
  - All subtrees will be delegated to this hierarchy.
  - The hierarchy will be owned by the cluster node user (`cluster-node`)
  - The child will call `unshare(CLONE_NEWCGROUP)` after being placed in the cgroup to make it the root of its cgroup namespace (to escape from the old namespace placed on it by systemd).
  - Then the parent will move the child process into the `/sys/fs/cgroup/dacha/cluster_node` hierarchy
    - This is done because cgroups only allows processes to exist in leaf hierarchies. Later we will creating additional leaf hiarchies under `/sys/fs/cgroup/dacha/` for each container.
1. For the remainder of the parent's lifetime, it will simply be running `waitpid` until the child exits and will then return the child's exit code.
1. Meanwhile the child will container with setup:
  - Calls `setsid()`
  - Calls `prctl(PR_SET_PDEATHSIG, SIGKILL)` : Will kill the child process when the 'main' (parent of root) process exits.
  - Calls `prctl(PR_SET_SECUREBITS, ..` to lock down to prevent cap increases (excluding setsuid/setsgid binary execution).
  - Calls `prctl(PR_SET_NO_NEW_PRIVS, 1)` to also prevent escalation via setuid/setgid binary executions.
  - Calls `mount /proc` (because we are now in a new pid namespace)
  - Calls `umask(077)` : Disallow FS group/world permissions
  - Start running the server.

Note that there are points in the above steps where the parent and child process must syncronize by waiting on each other to finish their steps. To achieve this, we show a unix socket between the two processes and block on reading a byte written by the other process when appropriate.

Later on when we want to start a container, we do the following:

1. Create pipes for stdout/stderr
1. Pick a new unused user and group id
1. Create a cgroup directory for the container (`/sys/fs/cgroup/dacha/[container-id]`)
  - (and set any relevant limits on the cgroup)
1. `clone(CLONE_NEWUSER, CLONE_NEWPID, CLONE_NEWNS, CLONE_NEWIPC | CLONE_INTO_CGROUP | CLONE_NEWCGROUP)`
1. The parent will set `/proc/[child-pid]/uid_map` = `/proc/self/uid_map` (same for group ids)
1. The child will create a new 'root' directory and set it up as follows:
  - Remount the current root ('/') as an MS_SLAVE mount.
  - Bind the new root directory to itself
  - Setup all the mounts needed by the container
  - Bind root as read only
  - Chroot
1. Then the child needs to transition to using the correct final permissions by: 
  - Calling `setsid()`
  - Calling `setgroups()` to keep only the explicitly requested groups.
  - Change the real/effective/saved uid/gids
  - Drop capabilities (regular and ambient)
1. Finally we can call `execve` to run the program.
  - Because we use CLO_EXEC in Rust on all file descriptors, this should hide any existing files from the child.
  - Typically this will actually run the `container_init` binary which wraps the main container binary.


#### Container Permissions

Each container will run using a unique newly allocated user id and group id. Restarting a container
for the same worker cause it to re-use the same user/group id (although users should not depend on this).

#### Persistent Volume

Volumes will be directories created on disk at `/opt/dacha/container/persistent/{name}`.

The owner of the directory will be the user running the container node binary. The group of the
directory will be a newly allocated group id which is reserved just for this volume. The directory
will have mode `660`.

The `S_ISGID` will be set on the volume directory to indicate that all files/directories under it
will inherit the same group id by default. Note: We don't currently prevent containers from
changing the group id of their files afterwards.

It is possible for the container to change the owner of a file in the volume to its own group id
and thus prevent attempt to prevent the container node from later deleting it. This won't stop the
container node from being able to delete the files as it has CAP_SYS_ADMIN in the user namespace
containing all of the user/group ids usable by containers.


#### File Permissions

- Blob Data:
 - Will be 644 as the main process should be able to write, but all containers must be able to read.
 - Eventually we should switch this to be a single group id per blob and switch this to 640 
- Container root directory stuff.
  - Is this directory even sensitive?


### Cluster Bootstrapping

TODO: Also describe resilience to cold starts (if all nodes suddenly power off and must be restarted).

### Manager

TODO

### Blob Registry

To support storing core cluster binaries (and user binaries/files), the cluster implements a self-standing blob registry. This means that we don't need to have a dependency on external image registries (e.g. Docker Hub) for bringing up the cluster from nothing.

Protocol:
1. When a user has a `JobSpec` they want to start on the cluster, they will list enumerate all blobs that the job requires.
  - This may require compiling the blob from source.
2. The user contacts the managers `Manager.AllocateBlobs` method with a list of `BlobSpec` protos defining the blobs that the user has and wants to use in the cluster
  - TODO: AllocateBlobs should probably create a provisional_replica_nodes entry in the metastore to make it aware that some nodes will soon have the blobs.
3. For all blobs not already present in the cluster, the `Manager` responds with the id of nodes to which the blobs should be uploaded initially.
  - TODO: Consider requesting replication to multiple nodes so that `StartJob` is less likely to stall if a node goes down (the main exception would be for jobs that are pinned to run on specific nodes). 
4. The user uploads the blobs directly to the aformentioned nodes.
  - Each `Node` implements a `BlobStore` service which is simply a CRUD-style bucket of blobs.
5. The manager will notice that the blobs were uploaded to each node and add a `BlobMetadata` entry to the metastore under `/cluster/blob/[blob_id]` which records which nodes have blob replicas.
6. The user will then send a `StartJob` request to the manager
7. The manager will call `StartWorker` on nodes.
8. When starting a worker, if a Node doesn't have a blob, it will look up the blob in the metastore to see where it is replicated and directly fetches it from the replica node's `BlobStore` service.
9. If a node had to fetch a blob, the manager notices this and adds that node as a new replica of that blob.
10. Over time, the manager checks the replication state of all blobs and does the following:
  - For all blobs actively in use by some worker in the cluster
    - Ensure the blob is replicated to at least 3 nodes and all nodes which are running a worker using it.
      - We ignore any node has hasn't been reachable for the past hour.
    - Delete the blob on all nodes not needed to achieve the above requirement.
    - TODO: Also want to avoid overwhelming single nodes with many blobs (must take into consideration storage space).
    - TODO: Periodically check that no nodes have corrupted data.
  - For all blobs that haven't been used in 1 week, delete the blobs from all nodes. 

### Monitoring/Logging

In this section, we'll describe how we track the history of state changes that occur in the cluster. 

For recording a change, the following identifiers should be kept in mind:

- Worker `name`
  - Note: We use indexes from 0-N where N is the number of replicas in a Job to create worker names so worker names will persist across many changes to a job. 
- Worker `revision`
  - Monotonically increases when the worker spec changes
  - Note: In the steady state all workers in a job will have the same `revision`.
- Worker `assigned_node`
  - Id of the node to which this worker is currently assigned.
  - Note: A worker may move between nodes (even while retaining the same `revision`) in scenarios such as node drains.
- Worker attempt `container_id`
  - A unique value is generated each time the worker stops and needs to be restarted.

High level cluster placement events such as workers being switched to a new revision, or moved to a new node are stored in the `Metastore` by the `Manager` job. In particular,

- `JobMetadata` entries get updated in place when new worker revisions are being added.
- `WorkerMetadata` entries get updated in place when updating to new worker revisions or when assigned to a new node.
  - For both of these, you can get the history of changes by looking through past `Metastore` version of each row.

Low level events that occur more frequently for each worker are stored by the `Node` runtime in its local storage:

- Local Node EmbeddedDB instance contains an `Events` table of the following form:
  - Row key: (`worker_name`, `timestamp`, `container_id`)
  - Row value is an `Event` object. We support the following types:
    - `WorkerStarted { worker_revision }`
    - `WorkerStopping { signal }`
    - `WorkerStopped { exit_code }`
    - `WorkerHeartbeat { passed: bool }`
  - The `Started` events can be used to delimit `Attempt`s of each worker.
- Log storage (stdout/stderr)
  - Stored in append only logs keyed by `container_id`.

Typical user journey:

- Lists all available running workers using `cluster list workers`
  - This can be fulfilled using the Metastore data
  - Optionally, we may want to show past revisions of workers or past assignments to specific nodes.
- User picks a worker they are interested in investigating.
- User runs `cluster events --worker=NAME` to retrieve recent changes
  - The CLI will find the current node on which a worker is running and retrieve the list of events from it directly.
- User wants to see the log output from a recent attempt of a worker
  - `cluster log --worker=NAME [--container_id=ID]`
  - Id a container_id is not specified, we will use the latest one (from the currently running worker). If a worker isn't currently running we will wait for one to start running.
  - This would be implemented by first finding the node to which the worker is assigned in the `Metastore` and then the logs themselves are retrieved directly from the node.

### Name Resolution

As mentioned in the user guide, we use a Rust client library to resolve where all 

TODO:

### Authentication

The cluster framework aims to leverage authenticated mTLS connections across all processes in the system. It achieves this by managing the generation and distribution of unique client/server X509 certificates to every worker and node.

Every entity (e.g. node/worker) will be granted its own X509 certificate which attests to its DNS name (e.g. `[node_id].node.[zone].cluster.internal` for a node).

Every cluster zone maintains its own PKI with the source of truth stored in the zone's metastore. In particular the metastore contains:

- List of root CA public certificates
  - This are restricted to signing certificates for a single `.[zone].cluster.internal` DNS suffix.
  - These will be long lasting 4 year+ certificates and infrequently used to sign anything.
- List of intermediate CA public certificates
- Certificate Revocation List (CRL) for each of the above.
- Log of all unexpired issues certificate ids
  - This is used in case we need to revoke certificates at a fine granularity.
  - Certificates associated with workers are additionally indexed by their assigned node.

Additionally the cluster maintains a special 'certificate-authority' job which serves the following functions:

- Stores the private keys associated with the root CAs mentioned above in local HSMs
- Accepts requests from nodes to sign new certificates
  - Rate limits signing requests:
    - Up to 10 QPS overall
    - Up to 5 QPS from a single client.
  - The CA is responsible for verifying the requester is allowed to sign the entity (e.g. a worker certificates can be requested by the node on which they are assigned to).
  - Before returned a signaed certificate, logs the certificate in the metastore
- Periodically (every few years), rotates its private key / certificate.

Note that because HSMs do not allow retrieving the stored private keys, adding a new replica must occur as follows:

- First a new worker is started as normal.
- When the CA binary starts up, if it depends that it has no private key stored, it will:
  - Locally create a new private key and uses it to self-sign a root CA certificate
  - Asks another CA instance to add the certificate to the metastore.

When a node starts up a worker, it is responsible for provisioning a certificate for it. This goes as follows:

- If the node already has a valid certificate for the worker on disk, it re-uses that.
- Otherwise, it creates one as follows: 
  - The node runtime generates a local private key (this never leaves the node machine) and a certificate request.
  - The node asks a CA instance to sign the certificate request
    - Note: This certificate expires in 2 days.
  - The node caches a copy of the certificate in local non-volatile storage.
- The node distributes the certificate information to the worker via a `/volumes/certificates` directory:
  - Contains the certificate, CA certs, and CRL list.
  - Periodically (every 12 hours) the node will refresh the certificate. The worker can detect this via file system events.
  - Similarly the node periodically retries the latest CRL/CA list.
- The worker starts up and reads the certificates volume to run its RPC server (and to configure any RPC clients).

Turning up a new node requires that:

- Another entity that has permission to turn up nodes contacts the CA to sign a new node certificate.
- The node is bootstrapped with an initial set of CA certificates and CRL on disk (otherwise it can't communicate with the metastore which itself should be using TLS).
- Node certificates last for 1 year and are refreshed every 6 months.

#### Bootstrapping

When creating a brand new cluster, we would follow the following procedure to bootstrap the PKI:

- On the user's machine that is doing the bootsrapping, we will:
  - Create a new root private key / self-signed public certificate
  - Use it to create:
    - A node certificate
    - One metastore worker certificate
    - One CA worker certificate.
  - Then the first node is bootstrapped with the node certificate and initial CA/CRL lists.
  - Tell the node to start a metastore and ca worker
    - The private key and certificate to use will be provided in the request.
      - Alternatively we could have a certificate request listing/approval API on the node.
  - Then we populate the metastore with:
    - Initial metastore/CA JobMetadata/WorkerMetadata
    - JobMetadata and WorkerMetadata for a Manager job running on the initial node.
    - CA/CRL metadata
    - ACLs on key ranges (`KeyRangeACL`s).
  - Finally we mark the Node as 'ready' and the Node will start enforcing that its worker set matches the metastore
    - This will also trigger the node to start the manager worker (generating the worker certificate on its own)

Note that by default, the Node and Metastore RPCs will all fail as no ACLs are setup yet. To enable this to function properly, the user boostrapping the cluster will create a special 'root' leaf certificate signed by the root CA:

- This certificate only lasts for 2 hours.
- This certificate has a special DNS name `root.[zone].cluster.internal`
  - When any RPC server sees this name, all RPC ACLs are disabled.

The usage of root certificates does not generally introduce a new security risk as anyone that is capable of creating a root certificate would also be able to forge a certificate with any other name. Creation of these root certificates requires direct access to a root CA private key and won't be exposed in the CA service's API.



#### Certificates

All X509 certificates used in the cluster:

- Use ECSDA
- Issuer and subject just contain a common name.
- Have extensions
    - Subject Key Identifier (not critical)
    - Authority Key identity (not critical)
    - Basic constraints (critical)
    - Subject Alternative Name (not critical)
- May have certificates
  - Name Constraints (critical) : To limit

#### RPC ACLs

By default, all RPCs to servers are disallowed. The metastore will store `ACLMetadata` rows which define who is allowed to use RPCs:

- By default, on Job creation, we will create an `ACLMetadata` object named after the job.
- The object defines RBAC style permissions:
  - Subjects are DNS names or DNS suffixes (e.g. `node.[zone].cluster.internal` to match all nodes in a zone).
  - Roles are defined as key-value tuples of labels like `{ method: "Service.Read" label: "custom_string" }`
    - Any of the values maybe regular expressions.
    - A single role is defined over a specific entity and set of permissions.
  - The `ACLMetadata` stores a list of assignments mapping subjects to roles.
- RPC servers will subscribe to up to one `ACLMetadata` object.
  - Before the RPC handler is executed, only standard labels like the RPC method will be checked.
  - During the RPC handler, the server implementor is responsible for verifying the reamining labels.

For example, we will use an RPC ACL to restrict who is allowed to create/modify/delete jobs. For example, we could give a single person access to create jobs in their own namespace as follows:

```
ACLMetadata {
  name: "job.system.manager"
  assignments: [
    {
      subject: "dennis.person.*.cluster.internal"
      roles: [{ rpc_method: "Manager.StartJob" job_name: "dennis\\..*" }]
    }
  ]

}
```


## Old

- Creating a new node
  - A role authorized to create nodes needs to contact the CA to generate a new machine cert.
  - Must bootstrap all new nodes with an initial copy of CA certs and CRLs
- Distributing worker certificates
  - Each node uses it's machine certificate to request a worker certificate for a CA
    - Private key created by node runtime and stays local
    - CA must verify that the node is running the requested worker
    - CA generates the certificate and records the serial number / name / expiration in the metastore
      - Must store enough info in the metastore to revoke the certificate if needed (e.g. revoking all certificates issues to a compromised node).
      - Must sign with a certificate that is more than 6 hours old (to ensure that all workers are aware of it).
      - Certificates are issued for 2 days.
  - Node also queries the metastore to get the latest:
    - List of important root/intermediate C
    - CRLs
  - Node writes all the files to a local directory shared with the worker via the filesystem.
    - Private keys will need to be encrypted at rest.
    - Also possible to keep private keys only in a tmpfs if the worker isn't 'low dependency'
  - Node periodically (every 12 hours) gets a new certificate for the worker and stores locally.


TODO: Explain the worker and node readiness protocols.



Implementing `exec`
-------------------

use `setns` to enter another process's namespaces?
- https://man7.org/linux/man-pages/man2/setns.2.html
- Should probably also use chroot.

TODO: After running chroot, we should drop capacbilities for doing that.


- Cluster Manager:
  - State:
    - List of all nodes in the cluster (each node will have info on resource limits)
    - List of all jobs 
    - Table of workers. Columns are:
      - Job Name
      - Worker Index
      - Assigned Machine
  - Operations:
    - Every once in a while:
      - Poll every Machine for the workers on it.
      - Check what workers should be on that machine
      - If a machine is not responsive, then we may have to 
    - Every once in a while:
      - Loop through all jobs and ensure that they are all 
    - Create a new job
      - Create an entry in the jobs table

- Metadata Key Format:
  - `/cluster/job/{job_name}`' JobMetadata
  - `/cluster/worker/{worker_name}`: WorkerMetadata
  - `/cluster/node/{node_id}`
  - `/cluster/manager/lock`
  - `/cluster/blob/{blob_id}`: BlobMetadata

  - `/cluster/node/{node_id}/worker/{worker_name}`
  - `/cluster/last_node_id`
  - For now, it will all be 
- Every node has an id
  - Incremented 


Network discovery protocals:
- SSDP (used by Hue)
- mDNS


- Must be resilient to single node restarts.


## DNS

Within a cluster, we will maintain a virtual name server that enables discovery between different workers using canonical names rather than ip addresses and port numbers.

All names in the cluster have the format:

- `[worker_id].[job_name].[user_name].[cluster_name].cluster.internal.`

All dot-separated components in the above name format must match the regexp `[a-z0-9_-]{1,63}`. The main exception is the `job_name` which may also contain a `.` character. All components are case-insensitive and their canonical form is lower case.

There will some special system jobs which always exist and have names of the following format:

- `[worker_id].metadata.system.[cluster_name].cluster.internal.`
  - Refers to the Metadata Store job for this cluster
- `[node_id].node.system.[cluster_name].cluster.internal.`
  - Special job which corresponds to every node machine in the cluster.

A full host name pointing to an individual worker will have the following DNS records:

- `A` record pointing to the ip address of the node running the worker.
- `SRV` record for each named port defined on the job/worker.

Additionally, a user may query an entire job using an entity name of the form:

- `[job_name].[user_name].[cluster_name].cluster.internal.`

Querying all DNS records for the above entity will result in receiving records for all workers in that job.

### Name Servers

The name servers which provide the aforementioned records are run by each `Metadata Store` worker and exposed by the `dns` named port. The network protocol is standard DNS. Therefore in order to query the cluster-level DNS records, a program must know:

1. The `cluster_name`
2. The `[ip address:port]` pair for at least one `Metadata Store` worker

If running on a node, the parameters will be propagated to each worker via enviroment variables named `CLUSTER_NAME` and `CLUSTER_NAME_SERVER`.

If not running on a node, but the cluster nodes are on the local network, the `Metadata Store` workers can also be contacted via mDNS on the standard 5353 port (visible at the node level). It is recommended that programs first use mDNS to query the records for the `system.metadata` job itself and then use regular uni-cast DNS to perform all remaining queries.


## Node Startup

- The node will acquire an IP address via DHCP.

- The node blocks on recovering the current time from a local RTC

- Next if the node is attached to a cluster,
  - It will use mDNS to find Metadata Store nodes.
  - It will update it's entry in the Metadata Store to reflect its current ip
    - Note that because a node may be hosting one of the metastore instance, the node will continue to start up even before it has registered itself. 
  - It will remember the ip addresses of all the Metadata Store replicas
  - Periodically the node will query one of the replicas in order to refresh it's list of ip addresses for the Metadata Store replicas.
  - The node runtime will host a DNS service which proxies / caches queries to the metadata store.
    - TODO: Eventually run the DNS service as a container.

- It will begin to start up any persistent workers assigned to it.
  - When asks are started, a CLUSTER_NAME_SERVER environment variable is added with the ip/port of the DNS service.

- Node will start an RPC server on port 10280

- It will now advertise itself as healthy.


Adding a job:
  - Check that it doesn't already exist,
  - In one transaction
    - Insert the job extra
    - Find nodes to assign it to.
    - Assign it to those nodes. (by creating Worker entries)
  - Finally contact the affected nodes and notify that 




We will support the following forms of addresses:


How to start a new job:
- Contact the metastore to find the manager
- Send a JobSpec to the manager (which will have all the state in RAM so should be )


CONTAINER_NODE_ID=XX
CONTAINER_NAME_SERVER=127.0.0.1:30001
    - DNS code needs to know where the metadata-store is located
    - Once we know that, we can query the metadata-store to get more


Port range to use:
    - Same as kubernetes: 30000-32767 per node
*/

Examples of USB cgroup propagation:

- https://www.zigbee2mqtt.io/information/docker.html
- https://git.lavasoftware.org/lava/pkg/docker-compose/-/merge_requests/7/diffs#386915d504f62f40813228b183d8b9bb1fff7433_0_39

/cluster/jobs/{,,,}


- About worker names
  - Using numbers wil lhelp make the ids consistnet across restarts
  - This is useful for storage services where we need to uniquely identify replicas
  - I'd like to prefer to not use numbers as numbers as shady




