# MCU Flashing Guide

This is a guide for how to flash a program binary to one of the supported MCU boards. In general, the input to these procedures will be a compiled binary in ELF format. Internally the tooling can convert this to UF2 or raw formats as needed, but users shouldn't have to worry about this.

## Boards/Chips

This section lists to specific dev boards / chips we support and how to process with flashing each of them. First read this section for your board/chip and follow the links to the other sections listed for it.

### nRF52

Note that we are mainly focused on supporting nRF52 chips with a USB peripheral for now. We will flash these chips with a custom bootloader which accepts UF2 over USB DFU based flashing. In particular we mainly support the following chips:

- nRF52840
- nRF52833

System registers are configured as follows:

- `PSELRESET` : configured so that the `nRESET` pin is a reset pin and not as a GPIO
- `NFCPINS` : configured so that NFC pins act as GPIOs
- `REGOUT0` : configured so that REG0 output is 3.3V
- `DCDCEN`/`DCDCEN0` : configured to default values of 0 so the LDO regulation is used.
    - Note that the `DCCH -> VDD` (for REG0) and the `DCC -> DEC4` (for REG1) inductors are not required.

When designing boards, if a lot of 3.3V power is required, then an external 3.3V regulatory is recommended. The XL1/XL2 crystal is optional but recommended (bootloader will run with all internal oscillators).

#### Custom Boards

All the custom boards in this repository with directly surface mounted MCU chips / modules have a TC2030 connector slot for flashing via SWD.

TC2030 Pinout

- Pin 1: VDD (3.3V)
- Pin 2: SWDIO
- Pin 3: nRESET
- Pin 4: SWDCLK
- Pin 5: GND
- Pin 6: Not connected for nRF boards / BOOTSEL on RP2040 boards

For nRF52 boards, we will flash the bootloader over `SWD`:

- Make one of [these](../boards/tc2030_adapter/index.md) 2x3 0.1" to 2x5 0.05" header adapters.
- Use a ribbon cable to connect the 2x5 header to the black magic probe.
- Use a [TC2030-IDC-NL](https://www.tag-connect.com/product/tc2030-idc-nl) cable to connect the adapter to the target board.
- Externally power the target board.

Later flashes of nRF52 boards can use the `UF2 over USB DFU` method.

For RP2040 boards, directly use the `Picoboot` method.

#### nRF52840 Dongle

General specs:

- Has a 32.768kHz crystal on XL1/2
- Does have inductors for REG0/REG1 to use the DC/DC converter.
- VDDH wired to USB 5V
- No external VDD regulator so will use the NRF52 to regulate down to 3.3V from USB.

TC2050 Pinout

- Pin 1: VDD
- Pin 2: SWDIO
- Pin 3: GND
- Pin 4: SWDCLK
- Pin 5: VBUS
- Pin 6-10: Not connected

Flashing:

- First flash the bootloader via `SWD`
    - Use a [TC2050-IDC-NL-050](https://www.tag-connect.com/product/tc2050-idc-nl-050) cable to go from TC2050 to the 0.05in pitch on a Blackmagic compatible probe.
        - MUST NOT have pin 5 in the cable since this is GND on the black magic probe and VBUS on the dongle.
    - Power the board via USB directly to the dongle.

- Then flash applications/new bootloaders via `UF2 over USB DFU`

#### Adafruit ItsyBitsy nRF52840 Express

[Adafruit Shop Link](https://www.adafruit.com/product/4481)

![Adafruit ItsyBitsy nRF52840 Express](boards/adafruit_itsybitsy_nrf52840/board.jpg)

General details:

- Has an external 5V USB/Battery to 3.3V regulator (max 600mA)
- Has a 32.768kHz crystal on XL1/2
- Internally uses the MDBT1 module

Flashing:

- First flash the bootloader via `SWD`
    - Use [this](../boards/tc2030_adapter/index.md) adapter to breakout a Black Magic Probe to 0.1" pins.
    - Connect the 0.1" pins on the adapter to the MCU board as follows:
        - VREF (3V), GND can be connected via regular female-female jumper wires
        - Use PCBite probes (or solder wires) to connect SWDIO/SWDCLK to the labeled test points on the back of the board.
    - Power applied to the board via USB directly to the board.

- Then flash applications/new bootloaders via the `UF2 over USB DFU` method.


#### Adafruit Feather nRF52840 Express

[Adafruit Shop Link](https://www.adafruit.com/product/4062)

![Adafruit Feather nRF52840 Express](boards/adafruit_feather_nrf52840/board.jpg)

General details:

- Same as the `Adafruit ItsyBitsy nRF52840 Express`

Flashing:

- Directly attach the SWD header on the board to a black magic probe and then 


#### XIAO nRF52840

![alt text](boards/xiao_nrf52840/back_pinout.webp) ![alt text](boards/xiao_nrf52840/front_pinout.webp)

Flash via SWD using the pins on the back. The process is similar to flashing the `Adafruit ItsyBitsy nRF52840 Express`.


### ATTiny

TODO

```
cargo run --bin flasher -- \
    target/attiny85/debug/avr.elf attiny \
    --reset_pin=18 --spi_device=/dev/spidev0.0
```


#### nRF52840 Dev Kit

TODO

```
openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program built/pkg/nordic/nordic_radio_dongle verify" -c reset -c exit
```

## Methods

### SWD

We support flashing boards via the ARM SWD protocol. This mainly requires connecting a flasher board to the VCC,GND,SWDIO,SWDCLK pins on the target device.

For flasher boards we recommend using a [Black Magic Probe](https://black-magic.org/index.html) though clones like the "Jeff Probe" on Amazon also work. BE SURE TO UPDATE THE FIRMWARE to the latest version to support all boards.

Things should generally be connected as:

- Computer -> Black Magic Probe via USB-C
- Black Magic Probe -> Target Board via 2x5pin header
- Power -> Target Board

We mainly recommend using SWD just for flashing the bootloader and then just using USB on the target device to do remaining flashing over DFU. 

Once everything is connected, here are example commands to flash the bootloader for an nRF board (modify the variables appropriately for the target chip type):

```
cargo run --bin builder -- build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader
cargo run --bin flasher -- built/pkg/nordic/nordic_bootloader blackmagic-swd
```

Note that once the bootloader is flashed, it can be reflashed using just a USB cable to the target board as described in the `UF2 over USB DFU` method. e.g.

```
cargo run --bin builder -- build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader
cargo run --bin flasher built/pkg/nordic/nordic_bootloader uf2-dfu
```

If power can't be supplied to the target board easily externally, you can add `--power_device` to the flasher command to power the board at 3.3V via the Black Magic Probe. WARNING: This is more dangerous and may break the board since not all chips are shipped from the factory with registers configured for 3.3V.

### UF2 over USB DFU

This protocol converts the application binary into a UF2 file and then transfers via the USB DFU protocol to the microcontroller. This protocol is currently implemented by our nRF52 bootloader and can be used as follows:

- Connect to the target device directly via USB
- If the device hasn't yet been flashed with an application firmware or the application is not responding, you may need to manually hit the RESET button once to enter the bootloader.
- If you successfully entered the bootloader, it should show up with id `8888:0001` under `lsusb`
- Then compile the application:
    - e.g. `cargo run --bin builder --  build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840`
- Flash the application:
    - e.g. `cargo run --bin flasher built/pkg/nordic/nordic_radio_dongle uf2-dfu`
- The MCU should automatically reboot into the application.

### Picoboot

TODO

