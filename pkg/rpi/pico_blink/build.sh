#!/bin/bash
set -e

export PICO_SDK_PATH=/home/dennis/workspace/dacha/third_party/rpi/pico-sdk
export PICO_BOARD=pico2

mkdir -p build
cd build
cmake ..
make -j$(nproc)
