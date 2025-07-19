# Cluster Runtime / Orchestration framework

This is a system for managing a fleet of machines and assigning work to run on them. This is similar to other systems like Google's Borg or Kubernetes.

## General Terminology

There are common terms you'll need to know to read this doc:

- `Cluster`: A set of `Node` machines on the same LAN.

- `Node`: A single Linux machine in a `Zone` which can run many `Workers` locally.
    - You would have multiple nodes if a single machine doesn't have enough resources to run all your workloads or you want redundancy.

- `Zone`: An isolated collection of `Node`s. One should typically have one `Zone`s per data center or geographic region. All machines in a zone are expected to be well connected in the network and each zone should be completely self sufficient in terms of workload management capabilities.
  - Multiple zones may exist on the same LAN with different names.

- `Worker`: A set of `Container`s (usually a single program) running in a shared resource envelope on a single `Node`.

- `Job`: A replicated set of `Worker`s with the same configuration (typically the workers will run on different nodes for redundancy).

## User Guide

This section describes the main user journeys for creating a cluster, updating it, and using it to run user workloads.

You should generally create one cluster for all machines in a local region / network. Each cluster is identified by a `Zone` name which should be unique across all instances you ever create.

To get started, pick a cluster name and define it as the `CLUSTER_ZONE` environment variant in your bash shell (this will just be used for running setup commands):

```
export CLUSTER_ZONE=home
```

The zone name should follow DNS name conventions (no spaces, dots, all lower case, etc.).

Note: Currently we assume that you are executing all `cluster` binary commands mentioned below in the same LAN as your cluster.

### Node Setup

Setting up a cluster involves first bootstraping the cluster using the first node machine and then later incrementally adding more nodes. The process is mostly identical for both cases.

WARNING: The first node by default hosts all the system processes for the cluster so must be up all the time.

Once you are done with this section, you will have a node that:

- Runs as a systemd service named `cluster-node`.
- Stores data in the `/opt/dacha/node` directory on the node machine.
- Runs an HTTPS RPC server on port 10400.

#### OS Configuration

We will present two sets of instructions:

1. For a 'Generic' machine: If you want to setup a node on your machine / Linux flavor of choice (also follow this path for development/testing on a desktop linux workstation).
2. For a 'Raspberry Pi' : Simplified instructions if you are going to be running on a Raspberry Pi.

##### Generic

Install a minimal Debian/Ubuntu installation onto the node. Minimal here means that you at least need `ssh`, `systemd`, and any kernel drivers for attached devices and networking.

Note that a single linux machine can only run a single instance of the node runtime running in a single zone.

**Linux packages:**

Run `sudo apt install uidmap`

**Configuration**

We assume that you've already set an IP for the machine and are able to connect securely over SSH (able to connect via an SSH client certificate and can authenticate the server certificate).

To simplify the setup, the main user on the machine should be named `cluster-user` and should be a 'sudoer'. When running a node on a local workstation, it is fine to just run all the tooling as any arbitrary `$USER`.

For identifying the machine, the scripting will rely on `/etc/machine-id` which we assume your OS has initialized on first boot to a random value.

We require that cgroups v2 are enabled for all subsystems on the machine running the node:

- Verifying whether cgroups v2 is setup correctly:
  - Running `cat /proc/cgroups` should show a hierarchy (second column) value of 0 for all rows.
  - Running `cat /proc/mounts | grep cgroup` should show a `cgroup2` mentioned at `/sys/fs/cgroup` (not in a 'unified' subdirectory).
- If this is not the case, then systemd must be reconfigured as follows:
  - Verify running at least version 240 of systemd (check using `apt list | grep systemd`)
  - Add `systemd.unified_cgroup_hierarchy=1 cgroup_no_v1=all` to the systemd / linux arguments
    - In Ubuntu this is done by appending these to `GRUB_CMDLINE_LINUX_DEFAULT` in `/etc/default/grub`
      and running `sudo update-grub` (then restart the computer).

##### Raspberry Pi

Follow the instructions [here](../rpi/index.md) to flash our custom image to all the SDcards you want to use in the cluster. It is recommended to flash while setting a static ethernet IP address (manually pick the next unused IP).

#### Installation

Next we will install the node runtime on the node. This script also handles setting up users, systemd services, etc. as needed.

If you want to run a node on your local machine, run the following command:

```
cargo run --bin cluster_cli -- \
  setup_node \
  --zone=$CLUSTER_ZONE \
  --local_node [--bootstrap --first_user_name=$USER]
```

To set up a remote machine (generic or Pi) adapt the following command:

```
cargo run --bin cluster_cli -- \
  setup_node \
  --zone=$CLUSTER_ZONE \
  --node_addr=10.1.1.4 \
  --ssh_args="-i ~/.ssh/id_cluster" [--bootstrap --first_user_name=$USER]
```

Important notes on the flags:

- Only specify `--bootstrap --first_user_name` before the first node is fully set up.
  - Also bootstraping is successful, future runs (even for the first node) should omit this to perform an inplace upgrade of the binaries without overwriting existing data files.
- `--first_user_name` : Specifies the name of the first admin user to create on the cluster. You will be automatically signed into this user on the machine running the setup commands.
  - You will also be prompted to enter a password in the terminal for future logging in as this user.

Repeat these steps for each node in your cluster.

#### Credentials Management

To perform future operations on the cluster, you will need to authenticated to the cluster. Locally testing server binaries also requires this. If you just bootstrapped your cluster, you should be able to run:

```
cargo run --bin cluster_cli -- list workers
```

to view all the system workers running in the cluster.

This works without any extra information because:

- The `~/.dacha/default_zone` file remembers the current zone you are working on.
- Credentials for your user are stored in `~/.dacha/zone/[zone]` (auto-signed in during bootstrapping).
  - Every zone also uses a unique root private key / certificate to sign all server certificates so a copy of the public key for your zone is also included in these credentials.

Note that user credentials expire after 1 month and you will need to re-login as mentioned below.

If you wanted to perform cluster operations from another computer, you will need to export the zone configuration data to a file:

```
cargo run --bin cluster_cli -- \
    save_zone_config zone_config.pb
```

`zone_config.pb` contains only public key metadata and can be safely distributed to other machines. On another machine you can run:

```
cargo run --bin cluster_cli -- \
    load_zone_config zone_config.pb \
```

to load the config into `~/.dacha/`. Then that machine can log in to a user:

```
cargo run --bin cluster_cli -- \
    login --user_name=$USER
```

(and enter the password on the command line).

Note that user login credentials expire after 1 month so users will need to re-login before then. It is recommended to restart your browser (Chrome or Firefox) completely (e.g. with `chrome://restart`) after performing a login or just restart your computer.

For backup purposes, it is recommended that you also backup the root credentials of the cluster (these are used to do operations like adding/updating nodes):

```
cargo run --bin cluster_cli -- \
    save_zone_config --include_root_creds secret_file.pb \
```

When using `--include_root_creds`, the file contains dangerous private keys and not be publicly accessible. The root credentials expire after 4 years.

TODO: Document how to rotate root credentials.

For services running inside the cluster, credentials are automatically managed and every worker gets its own TLS certificate. Any cluster that has been down for 1 month will become inaccessible due to some system certificates expiring.

#### Web Interfaces

All services in the cluster internally have a 'DNS name' and can be accessed via applications like Chrome/Firefox by going to their URLs (e.g. `https://...zone.cluster.internal`).

You can try this by right clicking on one of the name URLs printed out by:

```
cargo run --bin cluster_cli -- list workers
```

When you open it up in your web browser, you may be prompted to select a client certificate to use for authentication (subject should say something like `username.user.zone.cluster.internal`). The login script will configure Google Chrome to use this one by default so you may not see any dialog.

If your user is logged into the cluster, it should be running a DNS and local TCP proxy service (check with `systemctl status --user cluster-bridge`) that makes this possible. 

#### Done

At this point, you should have a useable cluster.

- Repeat the above steps (excluding bootstrapping) for any other machines that you want to manage in the cluster.
- If you want to expose your cluster to the internet (and be accesible outside your local network), follow this [ingress guide](./doc/ingress.md).
- If you are following the [main user guide](../../doc/user_guide.md) you can head back.
- Continue reading this page if you want to learn more about how to setup your own workloads.

## Sys Admin Guide

This section includes sys admin level information on how to manage the cluster.

### Terminology

In addition to the general terminology, we will also discuss:

- `Container`: Set of processes running inside of an isolated environment (using Linux cgroups,
  namespaces, chroot, etc.).
    - Individual `Container` instances will usually be identified by ramdom uuids and will be
      treated as ephemeral: if a `Worker` ever crashes and needs to be restarted, it will be assigned
      a fresh new container.

- `Manager`: Special process which manages the state of the cluster.
    - There will be a single `Manager` `Job` per `Zone` with one leader `Worker` at a time which ensures that the cluster is in a healthy state.
    - This process also hosts the user facing API for performing CRUD operations on `Job`s.

- `Metastore`: Strongly consistent and durable key-value store and lock service used to store
  the state of a `Zone`. There will be exactly one of these per `Zone`. Design documented [here](../datastore/src/meta/index.md).

- `CA` (or Certificate Authority): Another system job that issues credentials to users and workers.

- `Bundle`: Collection of files typically containing a binary + static assets and distributed as one or more `BundleBlob` archives.

- `BundleBlob`: A single usually large binary file identified by a hash. Blobs may also have a small amount of metadata such as a content type (e.g. tar or zip) to describe how they should be processed.

- `Volume`: Mounted path in a `Container`. Typically the source will be a `Bundle` or a persistent directory on the `Node`.

- `Attempt`: A single try at running a `Worker`. Typically this makes to one or more `Container`. Each `Attempt` is identified by a the start time of the first container in the `Attempt`.

### Common Operations

**Starting/updating a job:**

Jobs are started by providing a JobSpec protobuf file to the `cluster_cli start_job` command: e.g.

```
cargo run --bin cluster_cli -- start_job pkg/cluster/test/config/adder_server.job
```

If you look in the file referenced above, you will see that it:

- Specified the executable to run and the arguments to pass to it in the `args` 
- Defined `ports` (TCP ports) which should be reserved for the server running on the workers.
- Defines a single volume to mount which references the `//pkg/cluster/test:bundle` build target which is compiled and combines the executable and other files (e.g. HTML, JS, CSS files) into one tar file to deploy.

**List all jobs:**

After running a command like above, you can see all started jobs in the cluster by running the following command:

```
cargo run --bin cluster_cli -- list jobs
```

If you just started the `adder_server` example job, you should see it listed there. You can right click on its name to open the UI in your web browser. This should take you take a url like `https://adder_server.job.[zone].cluster.internal/`. Note that when connecting to a job directly, it will load balance between all the healthy workers of the job (if there is more than one).

**List all workers (and whether they are running):**

Run the following command to show all workers in the cluster:

```
cargo run --bin cluster_cli -- list workers
```

If you just starting the sample `adder_server`, you should see an entry that looks something like `adder_server.s0ths1bes91cm` where the last part `s0ths1bes91cm` is the worker id and the part before it corresponds to the job name. Similarly you can click on the worker name and visit it directly in your browser. This should take you to a URL like `https://s0ths1bes91cm.adder_server.worker.[zone].cluster.internal/`. Since this is the worker specific URL, this URL will always point to a single worker instance on a single node/machine.

Note that each worker has a globally unique 'worker id' and a single 'worker id' will always only be assigned to run on a single node (migration of replicas to other nodes will allocate new worker ids). Unlike Kubernetes/Borg, the worker ids are ALWAYS based on random integers. There is no concept of a 0th, 1st, 2nd, etc. worker as with Kubernetes stateful sets.

**List all attempts to run a single worker:**

A worker may sometimes fail and restart multiple times. To see a full log of restarts of a specific worker, run:

```
cargo run --bin cluster_cli -- events --worker_name=[worker_name]
```

**Print the log of a running worker:**

You can log the stdout/stderr logs of a worker by running

```
cargo run --bin cluster_cli -- log --worker_name=[worker_name]
```

Note that this will stream results and only stop when the current attempt has stopped.

If the worker is currently not running, you can view the last attempt's log by adding the `--latest_attempt` flag to the above command or specify a specific attempt number for something like `--attempt_id=1751144568908407`.

**Listing all nodes:**

You can find all nodes in the cluster by running:

```
cargo run --bin cluster_cli -- list nodes
```

While nodes are primarily identified by an 'id', you can also add labels (which are `key=value` pairs) to identify the nodes. e.g. assign a name to a single node as following:

```
cargo run --bin cluster_cli -- \
  labels set --node_id=[insert] "name=nas"
```

You can specify multiple labels by comma separating them. The `name` label is the recommended label to use for uniquely identifying nodes with human readable names.

### System Jobs

The system jobs are the Manager, CA, and Metastore. The cluster framework is designed such that these these jobs can temporarily be offline (for a few minutes) but things will break down if they  aren't available for a prolonged period of time:

- Without the manager, you can't add/remove/update jobs/workers.
- Without the CA, new workers/nodes can't start, you can't login to the cluster, and workers/nodes/users can't refresh their credentials near expiration.
- Without the metastore, none of the above is possible and additionally services can't make new 'DNS' queries to discover other services in the cluster.
  - The metastore itself can always be discovered since it broadcasts its location (IP address) to the entire LAN via multi-cast.

TODO: Document how to replicate system jobs.

### Users/Groups

The cluster framework fully manages user/workload authentication and restricting of ACLs between services. There are two main types of terminal entities which can make requests/calls to services (both types are identified internally by DNS names):

- Users (e.g. humans running on their personal computers).
- Workers (each worker is a distinct entity).
- Root 

We define a concept of a `Principal` which is simply a pattern string that matches one or more entities. Workers are always matched simply by specifying their job name (e.g. `dns:meta.system.job.local.cluster.internal` matches all workers in the metastore job in the current zone).

Additionally `Group`s can be defined which are lists of `Principals`. `Group`s can also be referenced in a `Principal` pattern so this allows for nested grouping as necessary.

Each server defines an `Principal` list for each of its endpoints (an HTTP path like `/` or `/rpc/Adder/Add` for an RPC) specifying who can access that endpoint. Some services will also have more complicated ACLs (e.g. the metastore manages ACLs per key range), but in general, most ACLs boil down to needing to have some entity in some principal/group.

The following are common principals that are used by standard services (sorted from lowest to highest privilege):

- `unauthenticated`: Matches clients that have no credentials. Only basic services required to facilitate user login allow access to this group.

- `authenticated`: Matches clients that have some credentials (a certificate signed by the cluster). This group is allowed to allow basic services like static web assets and public HTML pages.

- `group:cluster-clients`: This is a group which be default includes all jobs/workers. It grants the ability to resolve addresses of jobs/workers/nodes in the cluster and able to check ACLs.

- `group:cluster-readers`: Includes all of the `cluster-clients` privelages but also allows reading most non-secret metadata about the cluster (list of jobs, node descriptions/locations, etc.), basic monitoring metrics.

- `group:cluster-owners`: Includes all of the `cluster-readers` and grants access to most cluster wide operations (start/stop jobs, make users, edit groups etc.). This generally only includes human users.

The groups are arbitrary strings and more may be defined by specific services.

**Creating a user:**

New users can be created by anyone in the `cluster-owners` group. The first user created during cluster bootstrapping is one such user. To make a user, run a command like:

```
cargo run --bin cluster_cli -- create_user --user_name=tester --groups=cluster-readers
```
