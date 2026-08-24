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
mkdir -p build/out/DEBIAN

cat <<EOF > build/out/DEBIAN/control
Package: ar0234-driver-rpi-arm64
Version: 0.0.0
Section: kernel
Priority: optional
Architecture: arm64
Maintainer: Dennis
Description: Raspberry Pi AR0234 driver kernel module.
EOF

# 'dpkg' always tries to make backup links of old files when upgrading packages
# (which isn't supported on FAT32), so /boot/firmware files need to be deleted
# before we start the upgrade.

FIRMWARE_FILES=""
if [ -d "build/out/boot/firmware" ]; then
    FIRMWARE_FILES=$(cd build/out && find boot/firmware -type f | sed 's|^|/|')
fi

# Generate the preinst script. 
# We use an unquoted EOF here so that $FIRMWARE_FILES evaluates right now during the build, 
# while escaping \$1 and \$FILE so they evaluate later during dpkg execution.
cat <<EOF > build/out/DEBIAN/preinst
#!/bin/sh
set -e

# Auto-generated list of files destined for FAT32
FILES="
$FIRMWARE_FILES
"

if [ "\$1" = "install" ] || [ "\$1" = "upgrade" ]; then
    for FILE in \$FILES; do
        # Ignore empty strings resulting from formatting
        if [ -n "\$FILE" ] && [ -f "\$FILE" ]; then
            # Delete the file before dpkg tries to overwrite it and triggers a hard-link error
            rm -f "\$FILE"
        fi
    done
fi

exit 0
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

chmod 755 build/out/DEBIAN/preinst
chmod 755 build/out/DEBIAN/postinst

mkdir -p "$WORKSPACE_DIR/dist/pkg/rpi"

SOURCE_DATE_EPOCH=962409600 dpkg-deb -Z zstd -z 3 --root-owner-group --build build/out \
    $WORKSPACE_DIR/dist/pkg/rpi/ar0234-driver-rpi-arm64.deb

