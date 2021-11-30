

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



https://github.com/openocd-org/openocd/blob/master/tcl/board/nordic_nrf52_dk.cfg




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