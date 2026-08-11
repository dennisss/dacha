# Custom Pi Linux Kernel

We maintain a custom Linux kernel version for Raspberry Pis in the [//third_party/rpi/linux](/third_party/rpi/linux/) directory.

The kernel contains some extra features used by programs in this repo as well as trimming of features you'll likely never use on a Pi from the kernel to save space and improve boot performance.

## Compiling

This section will explain how to cross-compile the Kernel (e.g. from an x86 machine to run on the Pi). The prerequisites are to have the dependencies [referenced here](https://www.raspberrypi.com/documentation/computers/linux_kernel.html#cross-compile-the-kernel) installed on the local machine.

Then you just need to run the following in the root directory of this repo:

```
./pkg/rpi/scripts/compile_kernel.sh
```

NOTE: The above command should not require any user input. If it does, then likely the config patches broke.

The output of running this is the `third_party/rpi/linux/build/linux-kernel-dacha-rpi-arm64.deb` file.

Copy it to the pi-gen directory as follows:

```
mkdir -p third_party/pi-gen/data
cp third_party/rpi/linux/build/linux-kernel-dacha-rpi-arm64.deb third_party/pi-gen/data/linux.deb
```

Then return to the [instructions](/pkg/rpi/index.md) for compiling the image.

Backing up to a GCP bucket:

```
TIME=$(date +%Y-%m-%d)
gsutil cp "third_party/rpi/linux/build/linux-kernel-dacha-rpi-arm64.deb" "gs://da-manual-us/rpi-linux-builds/$TIME/linux-kernel-dacha-rpi-arm64.deb"
```

## Developing

The Linux code is just a standard Git sub module so can be modified as with normal sub modules.

On top of the kernel code, we maintain a patch file in `pkg/rpi/kernel_config_diff.txt` that gets applied to the Raspberry Pi defaults when `compile_kernel.sh` is run.

Also running the compile command once, you can modify the config using the Linux config UI as follows:

```
cd third_party/rpi/linux
make O=build/bcm2712 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- menuconfig
```

Then save the patches (run from the root of this repository):

```
cargo run --bin kernel_util -- \
    diff-configs \
    --base_path=third_party/rpi/linux/build/bcm2712/.config_base \
    --modified_path=third_party/rpi/linux/build/bcm2712/.config \
    --output_path=pkg/rpi/kernel_config_diff.txt
```

and then re-run the `compile_kernel.sh` command.

## Debugging Bloat

Run this in the linux directory after compiling the kernel to figure out what the biggest parts are:

```
find build/bcm2712 -name "built-in.a" | xargs aarch64-linux-gnu-size | sort -n -k4 | tail -n 200
```

