#!/bin/bash

set -euo pipefail

WORKSPACE_DIR="$PWD"
LINUX_DIR="$WORKSPACE_DIR/third_party/rpi/linux"

ARCH=arm64
CROSS_COMPILE=aarch64-linux-gnu-

cd third_party/ar0234-v4l2-driver

make KDIR="$LINUX_DIR/build/bcm2711" ARCH=$ARCH CROSS_COMPILE=$CROSS_COMPILE BUILD_DIR=build/bcm2711 all
make KDIR="$LINUX_DIR/build/bcm2712" ARCH=$ARCH CROSS_COMPILE=$CROSS_COMPILE BUILD_DIR=build/bcm2712 all

## Generating output artifacts.

KERNEL_VERSION_2711=$(make -s -C $LINUX_DIR/build/bcm2711 ARCH=$ARCH CROSS_COMPILE=$CROSS_COMPILE kernelrelease)
KERNEL_VERSION_2712=$(make -s -C $LINUX_DIR/build/bcm2712 ARCH=$ARCH CROSS_COMPILE=$CROSS_COMPILE kernelrelease)

rm -rf build/out
mkdir -p build/out/boot/firmware/overlays
mkdir -p build/out/lib/modules/${KERNEL_VERSION_2711}/updates
mkdir -p build/out/lib/modules/${KERNEL_VERSION_2712}/updates

cp build/bcm2712/ar0234.dtbo build/out/boot/firmware/overlays/

cp build/bcm2711/ar0234.ko build/out/lib/modules/${KERNEL_VERSION_2711}/updates/
cp build/bcm2712/ar0234.ko build/out/lib/modules/${KERNEL_VERSION_2712}/updates/

# tar -czvf build/out.tar.gz -C build/out .

## Making Debian package

mkdir build/out/DEBIAN

cat <<EOF > build/out/DEBIAN/control
Package: ar0234-driver-rpi-arm64
Version: 0.0.0
Section: kernel
Priority: optional
Architecture: arm64
Maintainer: Dennis
Description: Raspberry Pi AR0234 driver kernel module.
EOF

cat <<'EOF' > build/out/DEBIAN/postinst
#!/bin/sh
set -e

# Run depmod for all installed kernel module directories
for kver in /lib/modules/*; do
    if [ -d "$kver" ]; then
        version=$(basename "$kver")
        depmod -a "$version"
    fi
done
EOF

chmod 755 build/out/DEBIAN/postinst

dpkg-deb --root-owner-group --build build/out build/ar0234-driver-rpi-arm64.deb
