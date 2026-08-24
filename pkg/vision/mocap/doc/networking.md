# Optical Motion Capture : Networking Design

This page explains how the main mocap host computer communicates with the attached cameras.

## Goals

- The cameras by default should be plug and play out of the box with minimal user setup.
    - Things like mTLS authentication/encryption are disabled by default since these require additional setup for key management. 
- The software should not require sudo / 'Run as Admin'
    - This will mean we won't use some things like self hosted DHCP and we don't PTP time sync with the host machine by default since these operations generally require additional OS privileges.
- Efficiency / stability    
    - Low packet overhead (e.g. better to use IPv4 than IPv6 were possible to save bytes)
    - Avoiding mixing of mocap and non-mocap packets is important here for consistent system latency.

## Overview

The hardware topology should look like the following:

- The host should be connected via ethernet to a network switch
- The network switch will connect to all the cameras (either directly or via nested switches)
    - Currently for time sync, one of the cameras will be chosen as the 'leader' so must be connected via fast connections (dedicated switches) to all other cameras.
- There should be no router / DHCP server on the subnet containing the cameras.

On the software side, we will use [link-local addresses](https://en.wikipedia.org/wiki/Link-local_address) to communicate between the cameras and host. Cameras will be discovered using mDNS and DNS-SD.

## Camera Configuration

Each camera has a network hostname of the form `mocap-camera-[unique-id]`.

The cameras are running Linux using `systemd-networkd` for managing the ethernet interface. The cameras are configured to just request an IPv4 + IPv6 link local address.

Furthermore, they will respond to mDNS (DNS-SD) requests for the `_mocap._tcp.local.` service (this is handled by our camera supervisor program).

The networkd part is configured with the following config in `/etc/systemd/network/10-eth0.network`:

```
[Match]
Name=eth0

[Network]
DHCP=no
LinkLocalAddressing=yes

[Route]
Destination=224.0.0.0/4
Scope=link
```

## Host Configuration

The host computer could be running on any OS but we generally expect the OS to configure the ethernet interface with a link-local IPv4 address. Then the host software will find the cameras via mDNS by periodicially checking for new cameras every few seconds.

We generally assume that there is exactly one network interface configured with a link-local IPv4 address on the machine. The software will try to explicitly request all packets go in/out of that interface to avoid issues with the OS not routing packets how we want by default.

Note that for mDNS, we only query for 'PTR' records and check which IP addresses sent the records. So we don't bother recursively looking up the 'SRV' and 'A' records under the assumption that each camera has its own mDNS server.

How this all works varies a little bit on which OS is being used:

### macOS / Windows

On these, link-local IPv4 addresses are automatically assigned when plugging in an ethernet cable if DHCP fails so no configuration is required.

### Linux

Most mainstream distros do not default to auto-assinging link-local IPv4 addresses (only v6 link local addresses).

The software will instead do the following to setup the network:

- Look through all interfaces on the system
    - Ignore virtual or loopback devices.
- Try to find one that has only link local IP addresses (at least an IPv4 link local adddress)
    - If we find one, we can stop
- Else, attempt to find an interface with no IPv4 address
    - Assuming the system is using NetworkManager, we will run the following command to assign an ip to it:
        - `nmcli connection add type ethernet con-name mocap ifname [iface-name] ipv4.method link-local ipv6.method link-local`

## Camera-Host Time Sync

The current assumption is that the host machine does not have PTP capable hardware but will make a reasonable effort to sync the host with all cameras. The default time sync flow is as follows:

- Host gets real time from internet NTP.
- Host machine chooses a camera to act as the PTP leader and performs a 'pseudo-NTP' sync with it using software timestamps.
- 'Leader' Camera uses PTP to sync to all other cameras.

