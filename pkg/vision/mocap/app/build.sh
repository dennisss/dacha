#!/bin/bash

set -euo pipefail

WORKSPACE_DIR="$PWD"

cargo run --bin builder -- build //pkg/vision/mocap/manager:app

./pkg/vision/mocap/app/build_icons.sh

# TODO: Eventually need to run this in a docker container with an old libc version since
# Linux OSes are not forwards compatible.
cargo build --release --bin mocap_app

mkdir -p dist/pkg/vision/mocap/app/

tar --owner=0 --group=0 --transform 's/.*/mocap/' -czvf dist/pkg/vision/mocap/app/mocap-linux-x64.tar.gz target/release/mocap_app

cargo xwin build --target x86_64-pc-windows-msvc --release --bin mocap_app

mkdir -p out/mocap_app/windows
cp target/x86_64-pc-windows-msvc/release/mocap_app.exe out/mocap_app/windows/Mocap.exe

# For whatever reason, "-j" doesn't seem to work for me to clean up the file prefix in the zip file.
cd out/mocap_app/windows
rm -rf "$WORKSPACE_DIR/dist/pkg/vision/mocap/app/mocap-windows-x64.zip"
zip "$WORKSPACE_DIR/dist/pkg/vision/mocap/app/mocap-windows-x64.zip" Mocap.exe
cd "$WORKSPACE_DIR"

exit 0

export CMAKE_aarch64_apple_darwin=arm64-apple-darwin25-cmake
export CC_aarch64_apple_darwin=arm64-apple-darwin25-clang
export CXX_aarch64_apple_darwin=arm64-apple-darwin25-clang++
export AR_aarch64_apple_darwin=arm64-apple-darwin25-ar
export RANLIB_aarch64_apple_darwin=arm64-apple-darwin25-ranlib
export CXXSTDLIB_aarch64_apple_darwin=c++
PATH=$PATH:/home/dennis/workspace/osxcross/target/bin cargo build --bin mocap_app --release --target aarch64-apple-darwin