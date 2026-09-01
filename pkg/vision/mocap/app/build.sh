#!/bin/bash

set -euo pipefail

cargo run --bin builder -- build //pkg/vision/mocap/manager:app

./pkg/vision/mocap/app/build_icons.sh

# TODO: Eventually need to run this in a docker container with an old libc version since
# Linux OSes are not forwards compatible.
cargo build --release --bin mocap_app

mkdir -p dist/pkg/vision/mocap/app/

tar --owner=0 --group=0 --transform 's/.*/mocap/' -czvf dist/pkg/vision/mocap/app/mocap-linux-x64.tar.gz target/release/mocap_app

exit 0

cargo xwin build --target x86_64-pc-windows-msvc --release --bin mocap_app

# target/x86_64-pc-windows-msvc/release/mocap_app.exe


export CMAKE_aarch64_apple_darwin=arm64-apple-darwin25-cmake
export CC_aarch64_apple_darwin=arm64-apple-darwin25-clang
export CXX_aarch64_apple_darwin=arm64-apple-darwin25-clang++
export AR_aarch64_apple_darwin=arm64-apple-darwin25-ar
export RANLIB_aarch64_apple_darwin=arm64-apple-darwin25-ranlib
export CXXSTDLIB_aarch64_apple_darwin=c++
PATH=$PATH:/home/dennis/workspace/osxcross/target/bin cargo build --bin mocap_app --release --target aarch64-apple-darwin