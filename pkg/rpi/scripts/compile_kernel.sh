#!/bin/bash

set -euo pipefail

WORKSPACE_DIR="$PWD"
LINUX_DIR="$WORKSPACE_DIR/third_party/rpi/linux/"

cd "$LINUX_DIR"
mkdir -p build/
mkdir -p build/bcm2711
mkdir -p build/bcm2712

make O=build/bcm2711 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- bcm2711_defconfig
make O=build/bcm2712 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- bcm2712_defconfig

cp build/bcm2711/.config build/bcm2711/.config_base
cp build/bcm2712/.config build/bcm2712/.config_base


cd "$WORKSPACE_DIR"

cargo run --bin kernel_util -- \
    apply-config-diff \
    --diff_path="pkg/rpi/kernel_config_diff.txt" \
    --config_path="$LINUX_DIR/build/bcm2711/.config" \
    --output_path="$LINUX_DIR/build/bcm2711/.config" \
    --version="-rpi-2711-dacha"

cargo run --bin kernel_util -- \
    apply-config-diff \
    --diff_path="pkg/rpi/kernel_config_diff.txt" \
    --config_path="$LINUX_DIR/build/bcm2712/.config" \
    --output_path="$LINUX_DIR/build/bcm2712/.config" \
    --version="-rpi-2712-dacha"

### Kernels configured at this point

cd "$LINUX_DIR"
make O=build/bcm2711 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc)
make O=build/bcm2712 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc)

## Generating output artifacts

rm -rf build/out
mkdir -p build/out

make O=build/bcm2711 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- INSTALL_MOD_PATH=$LINUX_DIR/build/out -j12 modules_install
make O=build/bcm2712 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- INSTALL_MOD_PATH=$LINUX_DIR/build/out -j12 modules_install
rm $LINUX_DIR/build/out/lib/modules/*/build

make O=build/bcm2712 ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- INSTALL_HDR_PATH=$LINUX_DIR/build/out/usr -j12 headers_install

mkdir -p build/out/boot/firmware/overlays

gzip -n -c -9 build/bcm2712/arch/arm64/boot/Image > build/out/boot/firmware/kernel_2712.img
gzip -n -c -9 build/bcm2711/arch/arm64/boot/Image > build/out/boot/firmware/kernel8.img

cp build/bcm2712/arch/arm64/boot/dts/broadcom/*.dtb build/out/boot/firmware/
cp build/bcm2712/arch/arm64/boot/dts/overlays/*.dtb* build/out/boot/firmware/overlays/

# tar -czvf build/out.tar.gz -C build/out .

## Generating Debian Package (non-standard combined package)

# 'dpkg' always tries to make backup links of old files when upgrading packages
# (which isn't supported on FAT32), so /boot/firmware files must be deployed first
# to the root partition.
mkdir -p build/out/opt/linux
mv build/out/boot/firmware build/out/opt/linux

mkdir -p build/out/DEBIAN

cat <<EOF > build/out/DEBIAN/control
Package: linux-kernel-dacha-rpi-arm64
Version: 0.0.0
Section: kernel
Priority: optional
Architecture: arm64
Maintainer: Dennis
Description: Customized kernel for Raspberry Pis.
EOF

cat <<'EOF' > build/out/DEBIAN/postinst
#!/bin/sh
set -e

STAGING_DIR="/opt/linux/firmware"
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

STAGING_DIR="/opt/linux/firmware"
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

dpkg-deb --root-owner-group --build build/out build/linux-kernel-dacha-rpi-arm64.deb
