Container Runtime / Orchestration framework
===========================================




Terminology
-----------

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

- `Metadata Store`: Strongly consistent and durable key-value store and lock service used to store
  the state of the cluster. There will be exactly one of these for the entire cluster.




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
  - `/cluster/job/{job_name}`' JobProto
  - `/cluster/task/{task_name}`: TaskProto
  - `/cluster/node/{node_id}`
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
