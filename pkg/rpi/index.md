# Raspberry Pi Libraries

This directory contains libraries for building Raspberry Pi applications.

## Image

We provide a custom Raspbian Lite image configuration which other instructions assume you are using on your Raspberry Pis. Images can be generated using the `third_party/pi-gen` tool as described in the rest of this section. The image has a good default configuration in `third_party/pi-gen/config` that should NOT need to be edited.

Note: Only using a 64-bit Pi OS is supported right now.

**Custom Image Features**

Compared to the standard Raspbian Lite image, our image is meant to be headlessly provisioned in a cluster. Once the base image is flashed, it can be setup in a cluster using the instructions [here](../container/index.md). The unique features of our image is the following:

- Packages/users needed for running a cluster node are pre-installed.
	- Sets up a `cluster-user` user for manual inspection of the system.
	- Sets up a `cluster-node` user for running managed cluster binaries.
		- This user is allowlisted access to GPIO/I2C/SPI/USB/video devices via UDev rules.
- Disables unneeded features like HDMI output / Audio.
- Has a `periphmem` kernel module for allowing root-less access to PCM/clock peripherals in user space. 
- Has pre-installed `-dev` packages for compiling programs
	- These are not actually used on the Pi, but for simplicity are installed to have a consistent sysroot for cross-compilation.

**Step 1**: Create an ssh key that will be used to access all node machines.

- `ssh-keygen -t ed25519` and save to `~/.ssh/id_cluster`

**Step 2**: Build the image:

NOTE: If you don't want to build an image yourself, you can download the latest prebuilt one here: [2024-05-04-Daspbian-lite.img](https://storage.googleapis.com/da-manual-us/raspbian-builds/2024-05-04/2024-05-04-Daspbian-lite.img.gz).

Run the following commands to build a new Raspberry Pi SD Card image. This step requires that you have Docker installed:

```bash
PI_GEN_DIR=third_party/pi-gen
DATE="$(date +%Y-%m-%d)"

### Terminal 1
# In one terminal, start an HTTP cache (will record all the apt packages used).
cargo run --bin http_proxy --release -- \
	--port=9000 --cache_dir="${PI_GEN_DIR}/deploy/${DATE}-cache/"

### Terminal 2
cd $PI_GEN_DIR

# Build the base docker image
docker build --no-cache -t pi-gen-base:latest ./docker-base

# Build the pi image.
./build-docker.sh

# Internal command for pushing to GCS
gsutil -m cp -r "${PI_GEN_DIR}/deploy/${DATE}*" "gs://da-manual-us/raspbian-builds/${DATE}/"
```

**Step 3**: Flash the new image to all Pi SDCards.

If step #2 was successful, an image should be been written to `third_party/pi-gen/deploy/YYYY-MM-DD-Daspbian-lite.img.gz`.

This can be done using commands like the following:

```
cargo build --bin rpi_imager --release

sudo target/release/rpi_imager write \
    --image=$PWD/pi-gen/deploy/2024-05-04-Daspbian-lite.img.gz \
    --disk=/dev/sdb \
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

**Step 4** Test connecting

Once powered on, a Raspberry Pi will have a default hostname of `cluster-node`. If you look up the ip address of the Pi on your router, you can connect it with a command like the following:

```bash
ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.111
```

If following the [cluster setup guide](../container/index.md) then you can go back to that guide now.

## Cross Compiling

This section explains how to cross compile programs to run on the Raspberry Pi (specifically to run on the aforementioned image).

**Step 1**: Make sure you've installed the AArch64 dependencies mentioned in the [user guide](../../doc/user_guide.md).

**Step 2**: Set up a sysroot

We will extract the Raspbian image's root filesystem to a local directory so that we can reference headers/libraries in it during cross compilation.

Find the path to the uncompressed `.img` file you wrote to Pi SDCards (you may need to manually extract it) and then modify and run the below commands to extract it. The `output_dir` can't be changed:

```
cargo build --bin rpi_imager

sudo ./target/debug/rpi_imager extract \
	--image=/path/to/raspbian.img \
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

