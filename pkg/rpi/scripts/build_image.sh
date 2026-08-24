#!/bin/bash

set -euo pipefail

WORKSPACE_DIR="$PWD"
DATA_DIR=third_party/pi-gen/data

# Copy dependencies
rm -rf "$DATA_DIR"
mkdir -p "$DATA_DIR/"

cp dist/pkg/rpi/linux-kernel-dacha-rpi-arm64.deb "$DATA_DIR/linux.deb"
cp dist/pkg/rpi/ar0234-driver-rpi-arm64.deb "$DATA_DIR/ar0234.deb"

if [[ "$1" == "mocap" ]]; then
    cp dist/pkg/vision/mocap/mocap-camera.deb "$DATA_DIR/mocap-camera.deb"
    cp dist/pkg/vision/mocap/mocap-supervisor.deb "$DATA_DIR/mocap-supervisor.deb"
fi

cd third_party/pi-gen
./build-docker.sh -c "configs/$1"

cd "$WORKSPACE_DIR"
mkdir -p dist/third_party/pi-gen
mv "third_party/pi-gen/deploy/Daspbian-$1-lite.img.gz" "dist/third_party/pi-gen/Daspbian-$1-lite.img.gz"