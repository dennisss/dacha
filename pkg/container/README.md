# Cluster Runtime / Orchestration framework

This is a system for managing a fleet of machines and assigning work to run on them. This is similar to other systems like Google's Borg or Kubernetes.

## Terminology

- `Container`: Set of processes running inside of an isolated environment (using Linux cgroups,
  namespaces, chroot, etc.).
    - Individual `Container` instances will usually be identified by ramdom uuids and will be
      treated as ephemeral: if a task ever crashes and needs to be restarted, it will be assigned
      a fresh new container.

- `Node`: A single machine in a `Cluster` which has a fixed resource ceiling for running `Tasks`
  locally.

- `Cluster`: A collection of `Node`s 

- `Task`: A set of `Container`s running in a shared resource envelope on a single `Node`. Usually
  this will only be running a single `Container`.

- `Job`: A replicated set of `Task`s with the same configuration.

- `Manager`: Special process which manages the state of the cluster.
    - Will be replicas but there will only be one leader node at a time.
    - Runs in the cluster as a special `Job` that is bootstraped during cluster initialization and
      persists across cluster failures.
    - There will be a single `Manager` job per `Cluster`

- `Metastore`: Strongly consistent and durable key-value store and lock service used to store
  the state of the cluster. There will be exactly one of these for the entire cluster.

- `Blob`: A single usually large binary file identified by a hash. Blobs may also have a small amount of metadata such as a content type (e.g. tar or zip) to describe how they should be processed.

- `Bundle`: Collection of files typically containing a binary + static assets and distributed as a `Blob` archive.

- `Volume`:


## Blob System







Implementing `exec`
-------------------

use `setns` to enter another process's namespaces?
- https://man7.org/linux/man-pages/man2/setns.2.html
- Should probably also use chroot.

TODO: After running chroot, we should drop capacbilities for doing that.


Node Prerequisites
------------------

Linux packages:

- `sudo apt install uidmap`
  - Provides the `newuidmap` and `newgidmap` SETUID binaries for enabling us to support using a range of
    user ids for running containers while running the runtime binary as an unprivileged user.


Configuration:

- `sudo adduser --system --no-create-home --disabled-password --group cluster-node`
- Append to both `/etc/subuid` and `/etc/subgid` the line:
  - `container-node:400000:65536`
  - The above line assumes that no other users are already using this uid/gid range.


Raspberry Pi Files:

`crw-rw---- 1 root gpio 246, 0 Jul  7 04:17 /dev/gpiomem`


Next Steps:
- Step 1: Get the pi cluster back to runnable state:

- First just deploy a single node container-node

- Ensure logging still works.
- Design how the manager and DNS will work.
- Start cluster metadata store as a single replica.
- Start 

- Cluster Manager:
  - State:
    - List of all nodes in the cluster (each node will have info on resource limits)
    - List of all jobs 
    - Table of tasks. Columns are:
      - Job Name
      - Task Index
      - Assigned Machine
  - Operations:
    - Every once in a while:
      - Poll every Machine for the tasks on it.
      - Check what tasks should be on that machine
      - If a machine is not responsive, then we may have to 
    - Every once in a while:
      - Loop through all jobs and ensure that they are all 
    - Create a new job
      - Create an entry in the jobs table

- Metadata Key Format:
  - `/cluster/job/{job_name}`' JobMetadata
  - `/cluster/task/{task_name}`: TaskMetadata
  - `/cluster/node/{node_id}`
  - `/cluster/manager/lock`
  - `/cluster/blob/{blob_id}`: BlobMetadata

  - `/cluster/node/{node_id}/task/{task_name}`
  - `/cluster/last_node_id`
  - For now, it will all be 
- Every node has an id
  - Incremented 





Node Permissions
----------------

Each node in a cluster runs the `container_node` binary which implements the container runtime.
This binary runs as a non-root unpriveleged user starting with no capabilities (the `container-node` user in producer clusters).

When the binary starts, it will:
0. TODO: Should we first verify that all the real/effective/fs ids are the same?
0. Creates an `pipe()`
1. Run `clone()` with
  - CLONE_NEWUSER: Will be used to own the other namespaces created by this `clone()` call and grant permission to our unprivileged user to run as other users (see steps 2).
  - CLONE_NEWPID: Creates a new pid space for all containers. The main reason for this is to ensure that when the container_node binary dies, all containers it was running also die instead of ending up detached and unaccounted.
2. The parent process will call the `newuidmap` and `newgidmap` binaries to setup the `/proc/[pid]/uid_map` and `/proc/[pid]/gid_map` maps for the child process.
  - We will use an exact mapping from ids in the child user namespace to ids in the parent namespace for all ids in `/etc/subuid` and `/etc/subgid` for the current user in addition to the current user id itself. 
3. If the above fails, we will just exit.
4. Else, the parent will send a single byte through the pipe with value 0 to indicate that the child is fully setup.
5. For the remainder of the parent's lifetime, it will simply be running `waitpid` until the child exits and will then return the child's exit code.
6. When the child starts running, it will call `setsid()` and then block on getting a 0 through the pipe shared with the parent.

TODO: Set kill on parent death hook in child

7. At this point, we are all setup!
  - TODO: Set the secure bits to prevent capability escalation

8. When we want to start a new container
  - We will pick a new never before used userid and groupid
  - Call clone() with CLONE_NEWUSER, etc.
  - Similarly set the uid_map and gid_map from the parent while the child waits
    - In this case, we will contain to use an exact mapping of ids, but we will only expose a single user id and group id to the contianer
  - In the child we will call `setresuid()` and `setresgid()` to use this new uid/gid
  - Finally use `capset()` to clear all effective, permitted, and inheritable capabilities.

TODO: Use PR_SET_NAME in prctl to set nice thread names.
- TODO: Set PR_SET_NO_NEW_PRIVS
- PR_SET_SECUREBITS


Container Permissions
---------------------

Each container will run using a unique newly allocated user id and group id. Restarting a container
for the same task cause it to re-use the same user/group id (although users should not depend on this).

Persistent Volume
-----------------

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


File Permissions
----------------

- Blob Data:
 - Will be 644 as the main process should be able to write, but all containers must be able to read.
 - Eventually we should switch this to be a single group id per blob and switch this to 640 
- Container root directory stuff.
  - Is this directory even sensitive?



Data:




Network discovery protocals:
- SSDP (used by Hue)
- mDNS


- Must be resilient to single node restarts.


## DNS

Within a cluster, we will maintain a virtual name server that enables discovery between different tasks using canonical names rather than ip addresses and port numbers.

All names in the cluster have the format:

- `[task_index].[job_name].[user_name].[cluster_name].cluster.local.`

All dot-separated components in the above name format must match the regexp `[a-z0-9_-]{1,63}`. The main exception is the `job_name` which may also contain a `.` character. All components are case-insensitive and their canonical form is lower case.

There will some special system jobs which always exist and have names of the following format:

- `[task_index].metadata.system.[cluster_name].cluster.local.`
  - Refers to the Metadata Store job for this cluster
- `[node_id].node.system.[cluster_name].cluster.local.`
  - Special job which corresponds to every node machine in the cluster.

A full host name pointing to an individual task will have the following DNS records:

- `A` record pointing to the ip address of the node running the task.
- `SRV` record for each named port defined on the job/task.

Additionally, a user may query an entire job using an entity name of the form:

- `[job_name].[user_name].[cluster_name].cluster.local.`

Querying all DNS records for the above entity will result in receiving records for all tasks in that job.

### Name Servers

The name servers which provide the aforementioned records are run by each `Metadata Store` task and exposed by the `dns` named port. The network protocol is standard DNS. Therefore in order to query the cluster-level DNS records, a program must know:

1. The `cluster_name`
2. The `[ip address:port]` pair for at least one `Metadata Store` task

If running on a node, the parameters will be propagated to each task via enviroment variables named `CLUSTER_NAME` and `CLUSTER_NAME_SERVER`.

If not running on a node, but the cluster nodes are on the local network, the `Metadata Store` tasks can also be contacted via mDNS on the standard 5353 port (visible at the node level). It is recommended that programs first use mDNS to query the records for the `system.metadata` job itself and then use regular uni-cast DNS to perform all remaining queries.


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

- It will begin to start up any persistent tasks assigned to it.
  - When asks are started, a CLUSTER_NAME_SERVER environment variable is added with the ip/port of the DNS service.

- Node will start an RPC server on port 10280

- It will now advertise itself as healthy.


Adding a job:
  - Check that it doesn't already exist,
  - In one transaction
    - Insert the job extra
    - Find nodes to assign it to.
    - Assign it to those nodes. (by creating Task entries)
  - Finally contact the affected nodes and notify that 




The Metadata Store tasks each host a 

In order to query cluster-level DNS records, 




We will support the following forms of addresses:



- `node_id.node.[zone].cluster.internal`


- `task_index.job_name.task.cluster.local:port_name`







Metadata Tables
---------------


- The 


A job name must match /[A-z0-9_\-]{1,63}/



How to start a new job:
- Contact the metastore to find the manager
- Send a JobSpec to the manager (which will have all the state in RAM so should be )

- Making the manager atomic
  - The leader will 


/*

Bootstraping a cluster:
- Start one node
    - Bootstrap the id to be 1.
- When a task is started, the node will provide the following variables:
    -
- Manually create a metadata store task
    - This will require making an adhoc selection of a port
- Populate the metadata store with:
    - 1 node entry.
    - 1 job entry for the metadata-store
    - 1 task entry for the metadata-store
- When
    -
    -


What does the manager need to know:
- IP addresses of metadata server replicas
- The manager will have a local storage disk which contains a name to ip address:port cache
  that it can use for finding metadata servers (at least one of them must be reachable and then
  we can regenerate the cache).
- Alternatively, we could have a DNS service running on each node
- When a service wants to find a location, it



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





```
Start a node:
  cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto

Start a metastore instance on that node:
  cargo run --bin container_ctl -- start_task pkg/datastore/config/metastore.task --node=127.0.0.1:10250

Bootstrap the metastore:
  cargo run --package rpc_util -- call 127.0.0.1:30001 ServerInit.Bootstrap ''

Populate the metastore task and job into the store:
  (adds keys into '/cluster/task/system.meta.0' and '/cluster/job/system.meta')


Done!

cargo run --package rpc_util -- ls 127.0.0.1:30001

cargo run --bin container_ctl -- list --node=abc
```



