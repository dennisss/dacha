# Raspberry Pi Imager

This package contains an imager program for writing Raspberry Pi filesystem images to SDcards.

The imager assumes that the image being written has 2 partitions (first being the boot partition and second being a BTRFS root partition).

TODO: Need to add support for re-imaging a system that already has data:
- Need to preserve the host name, machine-id
- Need to preserve some data in directories like `/opt/dacha/`
- Need to preserve the SSH host keys