

Flashing a CC2531 sniffer:
- https://www.zigbee2mqtt.io/guide/adapters/flashing/alternative_flashing_methods.html

target = "thumbv7em-none-eabihf"

FLASH start at 0
RAM start at 0x20000000


`git clone git@github.com:openocd-org/openocd.git`
- Need the git version as ubuntu has an old version without nrf commands

```
sudo apt install --reinstall ca-certificates
sudo apt install libtool
./bootstrap
./configure
make
sudo make install
```

cargo build --package nordic --target thumbv7em-none-eabihf --release

openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program target/thumbv7em-none-eabihf/release/nordic verify" -c reset -c exit

~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb target/thumbv7em-none-eabihf/release/nordic
- https://openocd.org/doc/html/GDB-and-OpenOCD.html




Flashing via Raspberry Pi

- Follow Adafruit guide for adding Open OCD
  - https://learn.adafruit.com/programming-microcontrollers-using-openocd-on-raspberry-pi/overview
  - Pins 24 and 25 for SWD

Create board file:
  source [find interface/raspberrypi-native.cfg]
  bcm2835gpio_swd_nums 25 24
  transport select swd
  source [find target/nrf52.cfg]

^ May need to change to 'interface/raspberrypi2-native.cfg' depending on the pi.


cargo build --package nordic --target thumbv7em-none-eabihf --release

scp -i ~/.ssh/id_cluster target/thumbv7em-none-eabihf/release/nordic pi@10.1.0.67:~/binary

openocd -f nrf52_pi.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program /home/pi/binary verify" -c reset -c exit



Nice instructions for using CC2531 as Zigbee sniffer:
- https://www.zigbee2mqtt.io/advanced/zigbee/04_sniff_zigbee_traffic.html#with-cc2531


https://github.com/openocd-org/openocd/blob/master/tcl/board/nordic_nrf52_dk.cfg


Flashing using a Black Magic Probe

- Useful links:
  - https://github.com/blacksphere/blackmagic/wiki/Useful-GDB-commands
  - 

- Download GDB from:
  - https://developer.arm.com/tools-and-software/open-source-software/developer-tools/gnu-toolchain/gnu-rm/downloads
  - `sudo apt install libncurses5`

- `~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb`
- `target extended-remote /dev/ttyACM0`
- `monitor tpwr enable`: Power the device via 3.3V
- `monitor swdp_scan`
- `attach 1`
- `load target/thumbv7em-none-eabihf/release/nordic`


Updating black magic probe:
- `sudo apt install dfu-util`
- `sudo dfu-util -d 1d50:6018,:6017 -s 0x08002000:leave -D ~/Downloads/blackmagic-native.bin`


Programming the NRF dongle

- Info on the SWD interface:
  - https://infocenter.nordicsemi.com/index.jsp?topic=%2Fug_nrf52840_dongle%2FUG%2Fnrf52840_Dongle%2Fintro.html
  - Connector dimensions: https://www.tag-connect.com/wp-content/uploads/bsk-pdf-manager/TC2050-IDC_Datasheet_7.pdf

- There is a pogo adaptor:
  - https://www.thingiverse.com/thing:3384693
  - Pogo Pin P75-E2 Dia 1.3mm Length 16.5mm

- I have
  - P50-E2
    - http://digole.com/index.php?productID=521
  - Diameter: 0.68mm
  - Length 16.55mm
  - Full stroke: 2.65mm

My adapter:
- Switched from 0.7mm PogoPinDiam to 0.72
- Switched from 2mm to 6mm back extrusion
- From the back of the 2mm version, we want to have 11mm (7mm with the 6mm version)