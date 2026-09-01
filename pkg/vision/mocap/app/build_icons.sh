#!/bin/bash

set -euo pipefail

# Get the latest from https://imagemagick.org/download if you don't have the 'magick' command.
MAGICK="$HOME/apps/ImageMagick-7.1.2-30-gcc-x86_64.AppImage"

INPUT_SVG=pkg/vision/mocap/app/icon.svg
OUTPUT_DIR=out/mocap_app/icons

mkdir -p "$OUTPUT_DIR"

inkscape -w 2048 -h 2048 "$INPUT_SVG" -o "$OUTPUT_DIR/icon_2k.png"

# For windows
"$MAGICK" "$OUTPUT_DIR/icon_2k.png" -define icon:auto-resize=256,128,64,48,32,16 "$OUTPUT_DIR/icon.ico"

# For macOS
"$MAGICK" "$OUTPUT_DIR/icon_2k.png" \
  \( -clone 0 -resize 512x512 \) \
  \( -clone 0 -resize 256x256 \) \
  \( -clone 0 -resize 128x128 \) \
  \( -clone 0 -resize 64x64 \) \
  \( -clone 0 -resize 32x32 \) \
  \( -clone 0 -resize 16x16 \) \
  "$OUTPUT_DIR/icon.icns"

# For webview (read by the code)
"$MAGICK" "$OUTPUT_DIR/icon_2k.png" -resize 256x256 "$OUTPUT_DIR/icon.qoi"
