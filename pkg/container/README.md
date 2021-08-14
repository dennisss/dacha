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

- `sudo adduser --system --no-create-home --disabled-password --group container-node`
- Append to both `/etc/subuid` and `/etc/subgid` the line:
  - `container-node:400000:65536`
  - The above line assumes that no other users are already using this uid/gid range.


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