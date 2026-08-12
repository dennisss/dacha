#!/bin/bash

set -euo pipefail

# TODO: Must stay in sync with https://github.com/CleverCloud/systemd-services/blob/master/gen-host-keys

MOUNT_DIR="$1"

if [ -z "${1:-}" ]; then
    echo "Usage: $0 <path_to_sdcard_root>"
    exit 1
fi

if [ ! -d "$MOUNT_DIR" ]; then
    echo "Error: Directory '$MOUNT_DIR' does not exist."
    exit 1
fi

SSH_DIR="$MOUNT_DIR/etc/ssh"

mkdir -p "$SSH_DIR"

rm -f "$SSH_DIR/ssh_host_rsa_key"*
rm -f "$SSH_DIR/ssh_host_ecdsa_key"*
rm -f "$SSH_DIR/ssh_host_ed25519_key"*

ssh-keygen -t rsa -b 4096 -f "$SSH_DIR/ssh_host_rsa_key" -N "" -q
ssh-keygen -t ecdsa -f "$SSH_DIR/ssh_host_ecdsa_key" -N "" -q
ssh-keygen -t ed25519 -f "$SSH_DIR/ssh_host_ed25519_key" -N "" -q

chmod 600 "$SSH_DIR"/ssh_host_*_key
chmod 644 "$SSH_DIR"/ssh_host_*_key.pub
chown root:root "$SSH_DIR"/ssh_host_*
