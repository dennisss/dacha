#!/bin/bash
set -e

HEX_FILE="dist/pkg/media/camera/firmware/camera_ar0234_power.hex"
PORT="/dev/ttyUSB0"

if [ ! -f "$HEX_FILE" ]; then
    echo "Error: $HEX_FILE not found. Run ./pkg/media/camera/firmware/camera_ar0234_power/build.sh first."
    exit 1
fi

# The hex file contains both the flash image and the fuse configurations.
# We add '-H simple-unsafe-pulse' to explicitly toggle DTR/RTS to wake up the Adafruit HV UPDI Friend.
pymcuprog write -d attiny402 -t uart -u $PORT -f $HEX_FILE --erase --verify -H simple-unsafe-pulse
