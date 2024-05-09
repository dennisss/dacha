# User Guide

## Pre-requisites

To build and run the applications, we assume that you are running them on a Linux machine (or in a Linux VM/Docker container). The well tested OS is `Ubuntu 22.04 LTS` on an Intel/AMD x64 CPU.

To setup all dependencies for this project, do the following:

- Clone this repository into a local directory using your favorite `git` CLI/tool.
    - It is recommended to clone this into a stable/well-known location like `/home/$USER/workspace/dacha` since some steps will setup environment paths to this directory. 
- In this repository run the following to fetch submodules:
    - `git submodule update --init`
- Install needed Debian packages:
    - `sudo apt install ldd clang pkg-config uidmap libasound2-dev libglfw3-dev xorg-dev`
- Install Rustup per https://www.rust-lang.org/tools/install
- In the repository, run `rustup show` to install the ensure that the repository specific Rust version is installed.
    - This should install from the `rust-toolchain.toml` directory in the root of the repository.
- For supporting Raspberry Pi and embedded programs, also install these:
    - `sudo apt install gcc-arm-none-eabi g++-aarch64-linux-gnu`


## Cluster Setup

To run various services, we will setup a cluster of multiple machines on your LAN (e.g. Raspberry Pis):

- While setting up a cluster, it is worth being mindful of your router IP settings. It is recommended to ensure there is a separate IP range that isn't touched by DHCP to use for allocating cluster machine IP addresses. An example router LAN configuration (used by me) is shown below:
    - Router/Gateway IP: 10.1.0.1
    - Subnet Mask: 255.255.0.0
    - DHCP Range: 10.1.0.20 - 10.1.0.250
        - Used by regular non-managed home devices.
    - 10.1.1.1 - 10.1.100.255 : Implicitly unallocated range used by allocating cluster node static ips.
- Setup at least one Linux machine on your LAN that is always on.
    - If using Raspberry Pis, it is recommended to follow this [rack building guide](./pi_rack/README.md).
    - Note: OS setup will happen in the next step.
- Follow the [cluster setup user guide](../pkg/container/index.md) to build a managed cluster out of all the machines.


## Individual Applications

Below is a list of assorted applications that can be deployed on a cluster:

- [Stream deck based home automation controller](../pkg/app/home_hub/index.md)