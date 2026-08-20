# Optical Motion Capture : Camera OS Design

This page explains how the OS (Linux) is setup to faciliate 3 main objectives:

- Initial provisioning of a brand new camera
- Running the camera software
- Updating the camera OS/software over time.

Note that we are also targetting a very lightweight setup that is plug-n-play and uses minimum system resources so that <256MB of RAM systems can run all the software. That means that we use minimal software management layers and services on the camera side.

## Software/Config Management

Each camera essentially has the following data stored in it that we will need to push/update over time:

- Raspberry Pi Bootloader EEPROM
    - We do customize this for faster boot.
- SDCard/eMMC
    - 'Base Linux Image'
        - Contains core off the shelf packages like systemd
        - `systemd-networkd` configs are bundled in to setup the ethernet interface
        - Customizations like the custom linux kernel, camera drivers, and mocap camera software are bundled in as modular debian packages.
        - TODO: Make the default partition relatively big (ideally up to 4GB)
    - `/boot/firmware/camera_hardware.pb`
        - Contains hardware specific configs like the PCB hardware revision, factory calibration data, etc.
        - Note that this is stored on the FAT32 boot partition so that it is easy to read/write from any OS.
    - `/boot/firmware/config.txt`
        - This is the standard Pi file read by the bootloader to do initialize OS/hardware setup.
        - The base image will also have a `config-base.txt` file that contains base settings and is copied into `config.txt`
        - During provisiong and updates, `config.txt` is effectively always `config-base.txt` with customizations merged on top of it based on `camera_hardware.pb`.
    - `/etc/machine-id`

As a general rule, as much as possible that is not dynamic per camera is packed into the 'Base Linux Image' to make updates easier.

## Base Linux Image

The base linux image is currently built using the `pi-gen` framework with several customizations to improve performance. Some important notes are:

- We do aggresively tune the configs to specific boards (CM4, CM5, etc.) so that the boot time is as quick as possible.
- The OS partitions are strictly read only (only made writeable on explicit user update requests) to improve long term endurance and power loss safety.
    - This does imply that the tool that writes the image to the SDCard/eMMC is responsible for all 'first boot' tasks like resizing the root partition, generating a machine-id, etc.
- Standard `apt-get` is available but the majority of packages are not installed by default.

You can also read these two pages for additional references on the image setup:

- [//pkg/rpi/index.md](/pkg/rpi/index.md)
- [//pkg/rpi/doc/compute_module.md](/pkg/rpi/doc/compute_module.md)

## Building Image

Building the base linux image can be done using the following steps.

First build the camera software:

```bash
cargo run --bin mocap_deb -- build supervisor
cargo run --bin mocap_deb -- build camera
```

Then follow the image building instructions [here](pkg/rpi/index.md#building). The main change should be that the `build-docker.sh` command should change to the following:

```bash
./build-docker.sh -c configs/mocap
```

## Provisioning

Now we need to flash the image and all hardware specific configs.

Plug in the camera into the USB port of your computer (using the 5 pin header), then run the following to flash EEPROM customizations and mount the eMMC/SDCard as a disk on your computer (additional docs / prequisites [here](/pkg/rpi/doc/compute_module.md)):

```bash
pkg/rpi/scripts/provision_cm4.sh
```

Then run the following to flash the image to the eMMC/SDCard:

```bash
cargo build --bin mocap_imager --release

sudo target/release/mocap_imager \
    --image=third_party/pi-gen/deploy/2026-08-12-Daspbian-Mocap-lite.img.gz \
    --disk=mass-storage-gadget \
    --hardware_config="compute_module: PI_CM4_LITE compute_board_revision: 6"
```

## Startup

When the Linux OS starts the following will happen:

- `systemd` starts running
    - Everything after this point is a systemd service.
- `systemd-networkd` will start and assign an IP to the machine ([see this page](./networking.md))
- `sshd` will start (this is mainly used for debugging / development)
- `mocap-camera-supervisor` will start
    - This runs the mDNS server that will tell the host machine about the camera's existence.
    - This will has an RPC server running on port `81` which is used for all administrative tasks like updates.
    - This service is intentionally split off from the `mocap-camera` under the expectation that this service never crashes and is always around to help debug and rescue everything else.
- `mocap-camera` will start
    - This runs the actual camera software exposed over an RPC server at port `82`

Note that for simplicity, all services run as `root` (each of them requires substantial privileges anyway so there isn't much benefit to limiting permissions).

## Updates

In this section we will explain how we will go about updating the software running on cameras over time.

### Full Image

For most users, the preferred update strategy will be based on a 'full image' update approach built into the host app:

- Host app will pull the latest image from the web.
- Host app will check with the camera supervisor if this image is newer than the old one.
- Host app will compute the new values for non-trivial files like the `config.txt`
- Host app will transfer the image to the camera supervisor (which will temporarily enable writing to the FS)
- Camera supervisor will extract the image in-place
    - The image file is a `tar.gz` structured with one directory after another so it can be decompressed and streamed to the FS
    - Files like the machine-id and `config.txt` will be ignored.
    - A new `config.txt` will be written.
        - TODO: Also need to deal with changing partition ids in fstab.
- Host app will wait for the camera to come back online and check that the version has 

Note that we currently don't use an A/B partition scheme to avoid overcomplicating things since updates will be rare.

TODOs:
- MCU firmware updates
- EEPROM updates

### Partial Updates

- upload debian
- `dpkg -i [name].deb`
    - Need to make sure we have hooks to restart relevant services.

### config.txt / EEPROM Updates

TODO

### MCU Firmware Updates

TODO

## Logging

All the software runs as `systemd` services so use standard `systemctl` / `systemd-journald` for logging and monitoring.

Just note that since we use a read only file system, all logging stays in memory and is limited to 50MB of recent history.

## SSH

Assuming you know the IP address of the camera, you can login to it via SSH (username: `mocap`, password: `mocap`).
