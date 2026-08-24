#!/bin/bash

set -e

make -C pkg/vision/mocap/pps_divider

mkdir -p dist/pkg/vision/mocap
cp pkg/vision/mocap/pps_divider/build/pps_divider.bin dist/pkg/vision/mocap/pps_divider.bin
cp pkg/vision/mocap/pps_divider/build/pps_divider.elf dist/pkg/vision/mocap/pps_divider.elf
