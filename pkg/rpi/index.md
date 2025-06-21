# Raspberry Pi Libraries

This directory contains libraries for building Raspberry Pi applications.

## TLDR

Assuming you don't want to rebuild a Raspberry Pi system image from stratch, download a prebuilt one:

```
wget -P third_party/pi-gen/deploy/ https://storage.googleapis.com/da-manual-us/raspbian-builds/2025-04-27/2025-04-27-Daspbian-lite.img.gz
```

Then flash to your Pi's SDCards using the instructions in the `Flashing` section.

If you intent on later recompiling individual cluster binaries for the Pi (currently always needed), also run the commands in steps 1 and 2 of the `Cross Compiling` section.

## Image

We provide a custom Raspbian Lite image configuration which other instructions assume you are using on your Raspberry Pis. Images can be generated using the `third_party/pi-gen` tool as described in the rest of this section. The image has a good default configuration in `third_party/pi-gen/config` that should NOT need to be edited.

Note: Only using a 64-bit Pi OS is supported right now.

### Features

Compared to the standard Raspbian Lite image, our image is meant to be headlessly provisioned in a cluster. Once the base image is flashed, it can be setup in a cluster using the instructions [here](../container/index.md). Some of the features of our image is the following:

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

### Building

TLDR: Skip if you already downloaded the aforementioned precompiled `.img.gz` file

NOTE: If you don't want to build an image yourself, you can download the latest prebuilt one here: [2025-04-26-Daspbian-lite.img.gz](https://storage.googleapis.com/da-manual-us/raspbian-builds/2025-04-26/2025-04-26-Daspbian-lite.img.gz) and skip to the `Flashing` section.

Run the following commands to build a new Raspberry Pi SD Card image. These steps require that you have Docker installed:

```bash
PI_GEN_DIR=$PWD/third_party/pi-gen
IMG_DATE="$(date +%Y-%m-%d)"

mkdir -p "${PI_GEN_DIR}/deploy"

cargo build --bin http_proxy --release

# Start an HTTP cache (will record all the apt packages used).
# NOTE: The cache is only used for the pi image and not the base debian image.
cargo run --bin http_proxy --release -- \
	--port=9000 --cache_dir="${PI_GEN_DIR}/deploy/${IMG_DATE}-cache/" &

cd $PI_GEN_DIR

# Build the base docker image
docker build --no-cache -t pi-gen-base:latest ./docker-base
docker save pi-gen-base:latest | gzip > ${PI_GEN_DIR}/deploy/${IMG_DATE}-pi-gen-base.tar.gz

# Setup ip table rules so that the next docker build can only access the apt proxy.
# Note that ip table rules don't persist across system restarts.

# Print initial rules
sudo iptables -L DOCKER-USER --line-numbers

# Expected output of the above command:
#   Chain DOCKER-USER (1 references)
#   num  target     prot opt source               destination         
#   1    RETURN     all  --  anywhere             anywhere   

# Delete the existing rule
sudo iptables -D DOCKER-USER 1

# Create new rules.
# NOTE: This assumes that 172.17.0.1 is the docker0 ip (see 'ip addr').
# This is also hard coded in the 'pi-gen/config' file. We don't use
# 'host.docker.internal' since it isn't available in the chroot).
sudo iptables -I DOCKER-USER -i docker0 -d 172.17.0.1 -p tcp --dport 9000 -j ACCEPT
sudo iptables -A DOCKER-USER -i docker0 -j DROP

# Verify the rules are set up
sudo iptables -L DOCKER-USER --line-numbers

# Expected output of the above command:
#   Chain DOCKER-USER (1 references)
#   num  target     prot opt source               destination         
#   1    ACCEPT     tcp  --  anywhere             my-host-name            tcp dpt:9000
#   2    DROP       all  --  anywhere             anywhere

# Build the pi image.
# TODO: Pipe the IMG_DATE variable into this script to avoid regenerating the data.
./build-docker.sh

# Cleanup
sudo iptables -D DOCKER-USER 1
sudo iptables -D DOCKER-USER 1
sudo iptables -A DOCKER-USER -i docker0 -j RETURN

cd ../../
```

At this point you should have an `.img.gz` file in the `third_party/pi-gen/deploy` folder that you can use in the `Flashing` section.

Extra internal only commands for publishing the image (don't run these):

```
gsutil -m cp -r "${PI_GEN_DIR}/deploy/${IMG_DATE}*" "gs://da-manual-us/raspbian-builds/${IMG_DATE}/"
```

### Flashing

**Writing to SDCard**

We will now flash this image to a connected SDCard. The below tool will also handle setting up networking, SSH keys, expanding the filesystem, etc.

WARNING: Don't run the below commands before reading this entire section (just in case you need to add network setup flags).

```
cargo build --bin rpi_imager --release

# TODO: Modify the image and disk path to match your setup. 
sudo target/release/rpi_imager write \
    --image=$PWD/third_party/pi-gen/deploy/2025-04-27-Daspbian-lite.img.gz \
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

After flashing, you can insert the SDCard into your Raspberry Pi and power it on.

Once powered on, a Raspberry Pi will have a default hostname of `cluster-node`. If you look up the ip address of the Pi on your router (or use the statically configured one), you can connect it with a command like the following:

```bash
ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.111
```

If following the [cluster setup guide](../container/index.md) then you can go back to that guide now.

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
	--image=$PWD/third_party/pi-gen/deploy/2025-04-27-Daspbian-lite.img.gz \
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

