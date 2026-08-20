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

dpkg-deb --root-owner-group --build build/out build/linux-kernel-dacha-rpi-arm64.deb
