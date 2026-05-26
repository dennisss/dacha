#!/bin/bash

set -euo pipefail

WORKSPACE_DIR="$PWD"

SUBMODULE_DIR="$WORKSPACE_DIR/third_party/rpi/usbboot"

# Temp directory in which we will use to store the files we will inject into the CM5. 
PAYLOAD_DIR="/tmp/dacha/rpi_eeprom_payload"

BASE_EEPROM="$SUBMODULE_DIR/recovery5/pieeprom.original.bin"
EEPROM_CONFIG="$WORKSPACE_DIR/pkg/rpi/config/eeprom_cm5.txt"

echo "Staging EEPROM payload..."
rm -rf "$PAYLOAD_DIR"
mkdir -p "$PAYLOAD_DIR"

# Patch the stock EEPROM image with our custom config file. 
"$SUBMODULE_DIR/tools/rpi-eeprom-config" --out "$PAYLOAD_DIR/pieeprom.bin" --config "$EEPROM_CONFIG" "$BASE_EEPROM"
"$SUBMODULE_DIR/tools/rpi-eeprom-digest" -i "$PAYLOAD_DIR/pieeprom.bin" -o "$PAYLOAD_DIR/pieeprom.sig"

cp "$SUBMODULE_DIR/recovery5/bootcode5.bin" "$PAYLOAD_DIR/"

# We want the Pi to reboot after EEPROM flashing is done so that we can proceed to eMMC flashing.
cat <<EOF > "$PAYLOAD_DIR/config.txt"
[all]
recovery_reboot=1
EOF

echo ""
echo "======================================================"
echo "Flashing EEPROM..."
echo "======================================================"
sudo "$SUBMODULE_DIR/rpiboot" -d "$PAYLOAD_DIR"

echo ""
echo "======================================================"
echo "Entering mass-storage gadget mode..."
echo "======================================================"
sudo "$SUBMODULE_DIR/rpiboot" -d "$SUBMODULE_DIR/mass-storage-gadget64"

echo ""
echo "Done! You can now flash the Linux image."
