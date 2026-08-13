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

# 'dpkg' always tries to make backup links of old files when upgrading packages
# (which isn't supported on FAT32), so /boot/firmware files must be deployed first
# to the root partition. 
mkdir -p build/out/opt/ar0234
mv build/out/boot/firmware build/out/opt/ar0234

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

STAGING_DIR="/opt/ar0234/firmware"
TARGET_DIR="/boot/firmware"

if [ "$1" = "configure" ]; then
    echo "Deploying kernel files and hardware overlays to $TARGET_DIR..."
    
    if [ -d "$STAGING_DIR" ]; then
        cd "$STAGING_DIR"
        
        # Find all files in staging and mirror them to the FAT32 target
        find . -type f | while read -r FILE; do
            # Strip the leading './' from find output
            CLEAN_PATH="${FILE#./}"
            TARGET_FILE="$TARGET_DIR/$CLEAN_PATH"
            TARGET_SUBDIR=$(dirname "$TARGET_FILE")
            
            # Ensure the target directory structure exists (e.g., /boot/firmware/overlays)
            mkdir -p "$TARGET_SUBDIR"
            
            # Remove the existing file first to guarantee a clean overwrite on FAT32
            rm -f "$TARGET_FILE"
            
            # Copy the new file into place
            cp "$FILE" "$TARGET_FILE"
        done
    fi
fi

# Run depmod for all installed kernel module directories
for kver in /lib/modules/*; do
    if [ -d "$kver" ]; then
        version=$(basename "$kver")
        depmod -a "$version"
    fi
done
EOF

cat <<'EOF' > build/out/DEBIAN/prerm
#!/bin/sh
set -e

STAGING_DIR="/opt/ar0234/firmware"
TARGET_DIR="/boot/firmware"

# Triggered when the package is removed or about to be upgraded
if [ "$1" = "remove" ] || [ "$1" = "upgrade" ]; then
    echo "Cleaning up hardware overlays from $TARGET_DIR..."
    
    if [ -d "$STAGING_DIR" ]; then
        cd "$STAGING_DIR"
        
        find . -type f | while read -r FILE; do
            CLEAN_PATH="${FILE#./}"
            rm -f "$TARGET_DIR/$CLEAN_PATH"
        done
    fi
fi

exit 0
EOF


chmod 755 build/out/DEBIAN/postinst
chmod 755 build/out/DEBIAN/prerm

dpkg-deb --root-owner-group --build build/out build/ar0234-driver-rpi-arm64.deb
