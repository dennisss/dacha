#!/bin/bash
set -e

export PATH="$HOME/Downloads/avr/avr8-gnu-toolchain-4.0.0.52-linux.any.x86_64/avr8-gnu-toolchain-linux_x86_64/bin:$PATH"

RUSTFLAGS="-C opt-level=s -C panic=abort -C target-cpu=attiny402" cargo build --package camera_ar0234_power -Zjson-target-spec -Z build-std=core --target pkg/media/camera/firmware/camera_ar0234_power/avr-attiny402.json --release

mkdir -p dist/pkg/media/camera/firmware/
HEX_FILE="dist/pkg/media/camera/firmware/camera_ar0234_power.hex"
avr-objcopy -O ihex -R .eeprom target/avr-attiny402/release/camera_ar0234_power "$HEX_FILE"

# Remove EOF record
sed -i '/^:00000001FF/d' "$HEX_FILE"

# Extended Linear Address Record to set base address to 0x820000
echo ":02000004008278" >> "$HEX_FILE"

# BODCFG (Fuse 1) = 0x44 (LVL=2.6V at bits 7:5, ACTIVE=Enabled at bits 3:2) at address 0x0001
echo ":0100010044BA" >> "$HEX_FILE"

# OSCCFG (Fuse 2) = 0x01 at address 0x0002 (Logical 0x820002)
echo ":0100020001FC" >> "$HEX_FILE"

# SYSCFG0 (Fuse 5) = 0xC0 at address 0x0005 (Logical 0x820005)
echo ":01000500C03A" >> "$HEX_FILE"

# Append EOF record back
echo ":00000001FF" >> "$HEX_FILE"
