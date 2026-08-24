# Raspberry Pi Libraries

This directory contains libraries for building Raspberry Pi applications.

## TLDR

Assuming you don't want to rebuild a Raspberry Pi system image from stratch, download a prebuilt one:

```
wget -P third_party/pi-gen/deploy/ https://storage.googleapis.com/da-manual-us/raspbian-builds/2026-05-20/2026-05-20-Daspbian-lite.img.gz
```

Then flash to your Pi's SDCards using the instructions in the [Flashing](#flashing)` section.

If you intent on later recompiling individual cluster binaries for the Pi (currently always needed), also run the commands in steps 1 and 2 of the `Cross Compiling` section.

## Image

We provide a custom Raspbian Lite image configuration which other instructions assume you are using on your Raspberry Pis. Images can be generated using the `third_party/pi-gen` tool as described in the rest of this section. The image has a good default configuration in `third_party/pi-gen/config` that should NOT need to be edited.

Note: Only using a 64-bit Pi OS is supported right now.

### Features

Compared to the standard Raspbian Lite image, our image is meant to be headlessly provisioned in a cluster. Once the base image is flashed, it can be setup in a cluster using the instructions [here](../cluster/index.md). Some of the features of our image is the following:

- BTRFS root partition (so most data is read with data integrity checks).
- Sets up packages/configs needed for the cluster setup process to go smoothly
	- cgroups v2 enabled.
	- `cluster-user` main user with no password.
	- UDev rules to allowlist GPIO/I2C/SPI/USB/video device access.
- Disables unneeded features like HDMI output / Audio.
- Has a `periphmem` kernel module for allowing root-less access to PCM/clock peripherals in user space. 
- Has pre-installed `-dev` packages for compiling programs
	- These are not actually used on the Pi, but for simplicity are installed to have a consistent sysroot for cross-compilation.

### Development

TLDR: Skip this if you don't want to make changes to `pi-gen`.

When developing in custom `pi-gen` repository, we strictly build changes on top of the upstream `pi-gen` repository (no-merging). When we want to update to the latest upstream `pi-gen` head, we can run commands like the following to rebase on top of the latest head:

```
cd third_party/pi-gen
git remote add upstream https://github.com/RPi-Distro/pi-gen
git fetch upstream
git rebase upstream/arm64
```

### [Building](#building)

TLDR: Skip if you already downloaded the aforementioned precompiled `.img.gz` file

This section describes how to build custom Raspberry Pi image files.

This depends on having pre-built binaries for a few drivers. You can download pre-built binaries like this:

```
mkdir -p third_party/pi-gen/data/
wget -P third_party/pi-gen/data/ https://storage.googleapis.com/da-manual-us/rpi-linux-builds/2026-05-15/linux.tar.gz
wget -P third_party/pi-gen/data/ https://storage.googleapis.com/da-manual-us/rpi-ar0234-builds/2026-05-15/ar0234.tar.gz
```

Or you can build them yourself using the following instruction pages:

- [//pkg/rpi/doc/custom_kernel.md](/pkg/rpi/doc/custom_kernel.md)
- [//pkg/rpi/doc/ar0234_driver.md](/pkg/rpi/doc/ar0234_driver.md)

The image build process uses Docker and we use a custom network proxy to make it easier to reproduce images.

Run th following commands on your machine once to setup a custom docker network:

```bash
docker network create --internal pi-gen-apt-proxy
BRIDGE_IFACE="br-$(docker network inspect pi-gen-apt-proxy --format '{{.Id}}' | cut -c 1-12)"
sudo iptables -I INPUT -i $BRIDGE_IFACE -p tcp --dport 3142 -j ACCEPT
sudo iptables -A INPUT -i $BRIDGE_IFACE -j DROP
```

Then run the following commands to build a new Raspberry Pi SD Card image:


```bash
PI_GEN_DIR=$PWD/third_party/pi-gen

mkdir -p "${PI_GEN_DIR}/deploy"

# Start an HTTP cache (will record all the apt packages used).
# NOTE: The cache is only used for the pi image and not the base debian image.
cargo run --bin http_proxy --release -- \
	--port=3142 --cache_dir="third_party/pi-gen/deploy/cache/" &

# Build the base docker image
./pkg/rpi/scripts/build_image_base.sh

# Build the pi image.
./pkg/rpi/scripts/build_image.sh base
```

At this point you should have an `.img.gz` file in the `dist/third_party/pi-gen/` folder that you can use in the `Flashing` section.

### [Flashing](#flashing)

**Writing to SDCard**

We will now flash this image to a connected SDCard. The below tool will also handle setting up networking, SSH keys, expanding the filesystem, etc.

WARNING: Don't run the below commands before reading this entire section (just in case you need to add network setup flags).

```
cargo build --bin rpi_imager --release

# TODO: Modify the image and disk path to match your setup. 
sudo target/release/rpi_imager write \
    --image=$PWD/dist/third_party/pi-gen/Daspbian-base-lite.img.gz \
    --disk=/dev/sdc \
    --ssh_public_key=$HOME/.ssh/id_cluster.pub
```

If you want to connect to a WiFI network, modify and append the following arguments to the above command:

```
    --wpa_ssid=WIFI_NETWORK_NAME \
    --wpa_password=WIFI_NETWORK_PASSWORD
```

If you want to set a static ip address for the ethernet port, modify and append the following arguments:

```
	--ip_address=10.1.1.1 \
    --netmask=255.255.0.0 \
    --gateway=10.1.0.1
```

If you care about optimizing boot time, also specify the Pi model that this SDCard will be used for. Note that this will make it not work on other models but will speed up early boot.

```
	--hardware_model=pi5
```

If you care about boot time optimization, also update your EEPROM/bootloader according to the guidance on [this page](./doc/boot_time.md) after flashing the SDCard.

After flashing, you can insert the SDCard into your Raspberry Pi and power it on.

Once powered on, a Raspberry Pi will have a default hostname of `cluster-node`. If you look up the ip address of the Pi on your router (or use the statically configured one), you can connect it with a command like the following:

```bash
ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.111
```

If following the [cluster setup guide](../cluster/index.md) then you can go back to that guide now.

## Cross Compiling

This section explains how to cross compile programs to run on the Raspberry Pi (specifically to run on the aforementioned image).

**Step 1**: Make sure you've installed the AArch64 dependencies mentioned in the [user guide](../../doc/user_guide.md).

**Step 2**: Set up a sysroot

We will extract the Raspbian image's root filesystem to a local directory so that we can reference headers/libraries in it during cross compilation.

Modify the below commands to point to your image file and run them. The `output_dir` can't be changed:

```
cargo build --bin rpi_imager --release

sudo rm -rf /opt/dacha/pi/rootfs

sudo ./target/release/rpi_imager extract \
	--image=$PWD/dist/third_party/pi-gen/Daspbian-mocap-lite.img.gz \
	--output_dir=/opt/dacha/pi/rootfs
```

Note that using a mounted image directly doesn't work as many libraries like `/lib/aarch64-linux-gnu/libpthread.so.0` are setup as absolute symlinks which won't resolve correctly. The copy tool mentioned above will re-create the symlinks relative to the new rootfs directory.

**Step 3**: Compile

Ensure that you have a rust binary defined in a BUILD directory. e.g. in `pkg/rpi/streamer/BUILD` we have:

```python
rust_binary(
    name = "rpi_streamer",
    bin = "rpi_streamer"
)
```

Then you can build using the rpi64 config (which is internally configured to use the precreated sysroot):

```bash
cargo run --bin builder -- \
	build //pkg/rpi/streamer:rpi_streamer \
	--config=//pkg/builder/config:rpi64
```

## References

Cross Implementation
- https://github.com/cross-rs/cross/blob/main/docker/Dockerfile.aarch64-unknown-linux-gnu
- https://github.com/cross-rs/cross/blob/main/docker/toolchain.cmake

Example of how to make a memory driver:
- https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/char/broadcom/bcm2835-gpiomem.c
- https://github.com/raspberrypi/linux/blob/a90998a3e549911234f9f707050858b98b71360f/arch/arm/boot/dts/bcm270x-rpi.dtsi#L57

