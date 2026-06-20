# Compute Module Provisioning

In this doc, we want to outline a process for provisioning a compute module (e.g. a Raspberry Pi CM5) with a focus on eMMC based models (non-'lite') since the SDCard card path is much more straight forward.

We are going to be using [usbboot](/third_party/rpi/usbboot/Readme.md) for this so before starting, make sure to install any system dependencies documented for that.

**First Time Setup**

```
cd third_party/rpi/usbboot
git submodule update --init

make
```

**EEPROM Setup / Mounting**

Bridge the nRPIBOOT pin on your compute module board to GND and then connect the board to your computer via USB.

Run the following script which will configure the EEPROM to optimize eMMC boot and will then mount the CM5 as a mass storage device on your computer:

```
pkg/rpi/scripts/provision_cm5.sh
```

Or the following script for the CM4:

```
pkg/rpi/scripts/provision_cm4.sh
```

**Flashing**

And flash an image by modifying the below command (The special `disk` argument below allows auto-selecting the CM5's bootloader device):

```
cargo build --bin rpi_imager --release

sudo target/release/rpi_imager write \
    --image=$PWD/third_party/pi-gen/deploy/2026-05-20-Daspbian-lite.img.gz \
    --disk=mass-storage-gadget \
    --ssh_public_key=$HOME/.ssh/id_cluster.pub \
	--ip_address=10.1.1.9 \
    --netmask=255.255.0.0 \
    --gateway=10.1.0.1 \
	--hardware_model=cm5-regular
```
