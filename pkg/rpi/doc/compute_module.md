# Compute Module Provisioning

In this doc, we want to outline a process for provisioning a compute module (e.g. a Raspberry Pi CM5) with a focus on eMMC based models (non-'lite') since the SDCard card path is much more straight forward.

We are going to be using [usbboot](/third_party/rpi/usbboot/Readme.md) for this so before starting, make sure to install any system dependencies documented for that.

**First Time Setup**

```
cd third_party/rpi/usbboot
git submodule update --init

make
```

**Flashing Image**

Bridge the nRPIBOOT pin on your compute module board to GND and then connect the board to your computer via USB.

Then run the following to mount the Pi as a USB drive:

```
cd third_party/rpi/usbboot
sudo ./rpiboot -d mass-storage-gadget
```

And flash an image by modifying the below command (most likely the disk argument will be different):

```
cargo build --bin rpi_imager --release

sudo target/release/rpi_imager write \
    --image=$PWD/third_party/pi-gen/deploy/2026-04-27-Daspbian-lite.img.gz \
    --disk=/dev/sde \
    --ssh_public_key=$HOME/.ssh/id_cluster.pub \
	--ip_address=10.1.1.9 \
    --netmask=255.255.0.0 \
    --gateway=10.1.0.1 \
	--hardware_model=cm5-regular
```

**Configure EEPROM**

If you just flashed the image, unplug and plug back in the pi into your computer and then run:

```
cp pkg/rpi/config/eeprom_cm5.txt third_party/rpi/usbboot/recovery5/boot.conf

cd third_party/rpi/usbboot/recovery5
./update-pieeprom.sh
sudo ../rpiboot -d .
```