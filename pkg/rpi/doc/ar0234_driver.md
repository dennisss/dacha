# Pi AR0234 Driver

This is instructions for compiling the AR0234 linux driver for the Pi. This assumes that you have recently compiled the [linux kernel](./custom_kernel.md) for your custom Pi image so the linux sources are generated for compilation of modules.

Install the device tree compiler:

```
sudo apt install device-tree-compiler
```

Compile the driver:

```
./pkg/rpi/scripts/compile_ar0234.sh
```

The output of this is `third_party/ar0234-v4l2-driver/build/ar0234-driver-rpi-arm64.deb`

Copy it to the pi-gen directory as follows:

```
mkdir -p third_party/pi-gen/data
cp third_party/ar0234-v4l2-driver/build/ar0234-driver-rpi-arm64.deb third_party/pi-gen/data/ar0234.deb
```

Then return to the [instructions](/pkg/rpi/index.md) for compiling the image.


Backing up to a GCP bucket:

```
TIME=$(date +%Y-%m-%d)
gsutil cp "third_party/ar0234-v4l2-driver/build/ar0234-driver-rpi-arm64.deb" "gs://da-manual-us/rpi-ar0234-builds/$TIME/ar0234-driver-rpi-arm64.deb"
```