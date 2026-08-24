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

Then return to the [instructions](/pkg/rpi/index.md) for compiling the image.
