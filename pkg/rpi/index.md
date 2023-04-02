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

Run the following commands to generate the Raspberry Pi SD Card image. This step requires that you have Docker installed:

```bash
cd third_party/pi-gen
./build-docker.sh
```

**Step 3**: Flash the new image to all Pi SDCards.

If step #2 was successful, an image should be been written to `third_party/pi-gen/deploy/YYYY-MM-DD-Daspbian-lite.img`.

This can be done using commands like the following:

```
cargo build --bin rpi_imager --release

sudo target/release/rpi_imager write \
    --image=$PWD/pi-gen/deploy/2023-02-23-Daspbian-lite.img --disk=/dev/sdb \
    --wpa_ssid=WIFI_NETWORK_NAME \
    --wpa_password=WIFI_NETWORK_PASSWORD \
    --ssh_public_key=$HOME/.ssh/id_cluster.pub
```

If your Raspberry Pi will have a wired network connection, then the wpa flags are optional.

**Step 4** Test connecting

Once powered on, a Raspberry Pi will have a default hostname of `cluster-node`. If you look up the ip address of the Pi, you can connect it with a command like the following:

```bash
ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.111
```

## Cross Compiling

This section explains how to cross compile programs to run on the Raspberry Pi (specifically to run on the aforementioned image).

**Step 1**: Install cross-compilers using `sudo apt install g++-aarch64-linux-gnu`.

**Step 2**: Set up a sysroot

First manually mount the SDCard image onto your machined. We'll assume that the rootfs partition has been mounted to `/media/$USER/rootfs`.

The copy the rootfs to your computer's main filesystem:

```
sudo mkdir -p /opt/dacha/pi
sudo chown -R $USER:$USER /opt/dacha

cargo run --bin file --release -- \
	copy /media/$USER/rootfs /opt/dacha/pi/rootfs \
	--skip_permission_denied --symlink_root=/opt/dacha/pi/rootfs
```

Note that the mounted image can't be used directly as many libraries like `/lib/aarch64-linux-gnu/libpthread.so.0` are setup as absolute symlinks which won't resolve correctly. The copy tool mentioned above will re-create the symlinks relative to the new rootfs directory.

**Step 3**: Compile

Use a command like the following to compile a program:

```bash
PKG_CONFIG_PATH_aarch64_unknown_linux_gnu=/opt/dacha/pi/rootfs/usr/lib/aarch64-linux-gnu/pkgconfig \
PKG_CONFIG_SYSROOT_DIR_aarch64_unknown_linux_gnu=/opt/dacha/pi/rootfs \
BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu="--sysroot=/opt/dacha/pi/rootfs" \
CMAKE_TOOLCHAIN_FILE_aarch64_unknown_linux_gnu=$PWD/pkg/rpi/toolchain.cmake \
cargo build --target aarch64-unknown-linux-gnu --release --bin rpi_streamer
```

## References References

Cross Implementation
- https://github.com/cross-rs/cross/blob/main/docker/Dockerfile.aarch64-unknown-linux-gnu
- https://github.com/cross-rs/cross/blob/main/docker/toolchain.cmake


Example of how to make a memory driver:
- https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/char/broadcom/bcm2835-gpiomem.c
- https://github.com/raspberrypi/linux/blob/a90998a3e549911234f9f707050858b98b71360f/arch/arm/boot/dts/bcm270x-rpi.dtsi#L57

