# User Guide

## Pre-requisites

To build and run the applications, we assume that you are running them on a Linux machine (or in a Linux VM/Docker container). The well tested OS is `Ubuntu 22.04 LTS` on an Intel/AMD x64 CPU.

To setup all dependencies for this project, do the following:

- Clone this repository into a local directory using your favorite `git` CLI/tool.
    - It is recommended to clone this into a stable/well-known location like `/home/$USER/workspace/dacha` since some steps will setup environment paths to this directory.
- In this repository run the following to fetch submodules:
    - `git submodule update --init`
- Install needed Debian packages:
    - `sudo apt install clang pkg-config uidmap libnss3-tools libasound2-dev libglfw3-dev xorg-dev`
- Install Rustup per https://www.rust-lang.org/tools/install
- In this repository, run `rustup show` to install the ensure that the repository specific Rust version is installed.
    - This should install from the `rust-toolchain.toml` directory in the root of the repository.
- For supporting Raspberry Pi and embedded programs, also install these:
    - `sudo apt install gcc-arm-none-eabi g++-aarch64-linux-gnu`
- For supporting AVR applications:
    - `sudo apt install gcc-avr avr-libc`
- For viewing/developing the PCB files in this repository:
    - Follow the KiCad setup instructions [here](/pkg/cnc/kicad/index.md).

## Cluster Setup

The majority of programs in this repository are designed to at least in some part run in a cluster of Linux machines (at least 1 machine on the local network or in the cloud). We have our own Kubernetes-like runtime that orchestrates program execution. This section will walk through how to set up a 'cluster' of some number of machines.

### SSH

For cluster setup, we expect that each machine can be accessed via SSH for configuration. Set up a global SSH private key for your machines by running `ssh-keygen -t ed25519` and saving it to `~/.ssh/id_cluster`.

### Networking

When setting up a cluster on your home LAN, it's important to be aware of your used IP ranges / router settings. It is recommended to reserve a separate IP range for static IPs (a range that doesn't overlap with the DHCP range on your router) for allocating static IPs to cluster machines. 

An example router LAN configuration (used by me) is shown below:

- Router/Gateway IP: 10.1.0.1
- Subnet Mask: 255.255.0.0
- DHCP Range: 10.1.0.20 - 10.1.0.250
    - Used by regular non-managed home devices.
- 10.1.x.x : Implicitly unallocated range used by allocating cluster node static ips.

### Machine Templates

You're free to use any Linux based machine(s) to set up the cluster, but here are my notes from a few well lit builds I actively run in my home:

- [Generic Raspberry Pi](../pkg/rpi/index.md)
    - Install an SDCard image via these instructions.
- [Raspberry Pi 'B form factor' Server Rack Build Guide](./pi_rack/README.md)
    - Follow the generic pi instructions for installing an OS on top of this.
- [HL15 / AMD Epyc Based NAS](../pkg/cluster/machines/nas/index.md)
    - This is what I use for general file storage and backup.

### Setup

Follow the [cluster setup user guide](../pkg/cluster/index.md) to build a managed cluster out of all the machines.

## Individual Applications

Below is a list of assorted applications that can be deployed on a cluster:

- [Label Print Server/UI](../pkg/things/labeler/index.md)
- [Stream Deck based home automation controller](../pkg/app/home_hub/index.md)


