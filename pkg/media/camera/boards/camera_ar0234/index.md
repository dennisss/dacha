# AR0234 Camera Board

This is a board for attaching an AR0234 camera sensor to a Raspberry Pi 22-pin connector (2 or 4-lane MIPI).

- PCB Stackup: 6-layer; `JLC06161H-3313`
- Need to use via-in-pad processing (capped/plated over vias)

Power consumption (based on datasheet):

- Chip Power Consumption: <420mW
- Board Power Input
    - Typical: 0.23A on 3.3V rail
    - Peak: 0.378A on 3.3V rail (~0.25A on 5V)

## TLDR

Please read all the instructions BEFORE you start doing stuff:

- Buy the parts listed in the [BOM](#bom) + PCBs
- [Flash the MCU](#flashing)
- Assemble the PCB
- [Do testing](#testing)

## [BOM](#bom)

- Sensor
    - https://www.digikey.com/en/products/detail/onsemi/AR0234CSSM00SUKA0-CP/15860697
- Connector
    - https://www.digikey.com/en/products/detail/hirose-electric-co-ltd/FH34SRJ-22S-0-5SH-50/5132526
    - 24mm tape
- Crystal
    - https://www.digikey.com/en/products/detail/ecs-inc/ECS-2520MV-240-DN-TR/16578318
- 1.2V Regulator
    - https://www.digikey.com/en/products/detail/texas-instruments/TLV75512PDYDR/22531581
- 1.8V Regulator
    - https://www.digikey.com/en/products/detail/texas-instruments/TLV74318PDBVR/7593922
- 2.8V Regulator
    - https://www.digikey.com/en/products/detail/onsemi/NCP163ASN280T1G/10064750

- Level Shifting Transistor
    - https://www.digikey.com/en/products/detail/onsemi/BSS138/244210

- Microcontroller
    - 12mm tape
    - https://www.digikey.com/en/products/detail/microchip-technology/ATTINY402-SSN/9947535

- 0.1uF 16V 0402 Capacitor
    - https://www.digikey.com/en/products/detail/murata-electronics/GRM155R71C104KA88J/2610892
    - 0.5mm +/- 0.05 height
- 1uF 10V 0402 Capacitor
    - https://www.digikey.com/en/products/detail/murata-electronics/GCM155C71A105KE38D/6012263

- 1.5K 0402 Resistor
    - https://www.digikey.com/en/products/detail/yageo/RC0402FR-071K5L/726519
- 10K 0402 Resistor
    - https://www.digikey.com/en/products/detail/yageo/RC0402JR-0710KL/726418

- 1uF 16V 0603 Capacitor
    - https://www.digikey.com/en/products/detail/yageo/CC0603KRX7R7BB105/2833611
- 10uF 10V 0603 Capacitor
    - https://www.digikey.com/en/products/detail/samsung-electro-mechanics/CL10A106KP8NNNC/3886850


## [Flashing](#flashing)

The MCU on the PCB has to be programmed with the firmware for powering on/off the camera sensor. The code that runs in the MCU is located in [this directory](/pkg/media/camera/firmware/camera_ar0234_power).

It is recommended (and safest) to flash it before you solder it to the PCB though you can also program it after it is on the PCB.

To get the firmware, either, fetch a prebuilt blob:

```bash
cargo run --bin source_control -- fetch dist/pkg/media/camera/firmware/camera_ar0234_power.hex
```

Or build it from source:

```bash
./pkg/media/camera/firmware/camera_ar0234_power/build.sh
```

Then, for flashing it, you will need:

- [USB UPDI Programmer](https://www.adafruit.com/product/5893) : High voltage one is ideal so that you can reprogram later if needed. Non-high voltage ones also work but you will only have one shot at programming.
- For programming before soldering:
    - [SOIC-8 150mil socket](https://www.amazon.com/dp/B0DY67PH8R)
- For programming after soldering
    - Either use the testing board to be mentioned [later](#testing) or get a SOIC-8 clip like [one of these](https://www.digikey.com/en/products/detail/pomona-electronics/5250/745102)

You will need to connect the programmer to 3.3V, GND and the ENABLE_3V3 (PA0) pin on the MCU.

Then you can run the following command to program the MCU:

```bash
./pkg/media/camera/firmware/camera_ar0234_power/upload.sh
```

Note: Running this command requires that you have `pymcuprog` installed.

## [Testing](#testing)

After you have soldered the board, you should always check that there is no continuitiy between GND, 3.3V, 1.8V, 1.2V, 2.8V rails on the board (with a multimeter).

Then we have a standalone testing board whos job is to verify that there are signs of life from the sensor (before wiring up the camera to an expensive Raspberry Pi for final testing):

- Make the board in [this directory](/pkg/media/camera/boards/camera_tester/)
    - The enclosure for this board is `camera-tester-holder.stl`
    - The 5-pin header connects to this [CSI breakout](https://www.amazon.com/dp/B09VPKWL1G)
    - The 3.3V/GND pin header should go to a current limited PSU (0.5A max)
    - The other header can be used to flash the MCU if you haven't already done so (no need to power the MCU from the programmer if you already powered it from the other header)
- Flash the nRF MCU with the [nordic_radio_dongle firmare](/pkg/peripherals/doc/flashing.md)
- With the MCU connected to your computer, run the testing program:
    - `cargo run --bin mocap_cli -- test_camera_board`
- Once you have plugged in the camera, power on your 3.3V PSU and type `y [enter]` in the CLI
- If it reads out a register value and doesn't error out, then it is successful.

## Notes

- EXTCLK : 24Mhz on the reference board. (VDD_IO / 1.8V)
    - ECS-2520MV-240-DN-TR

- Inputs (3.3V):
    - TRIGGER
    - SCLK
    - SDATA (bidi)
        - Note that the Pi probably has pullups already on one side.

- Power Sequencing
    - (1) Wait for ENABLE input (from Pi) (active high)
    - (2) Turn on 2.8V (analog power)
    - Wait 100us
    - (3) Turn on 1.8V
    - Wait 100us
    - (4) Turn on 1.2V
    - Wait until stable
    - (5) Turn on EXTCLK
    - Wait?
    - (6) Assert RESET_N (pull low) for at least 1ms
    - After ~160000 EXTCLKs for init to finish

- Use an MCU for sequencing
    - ATTINY40-MMHR

- Power Requirements:
    - 2.8V : 60mA typical, 115mA peak
        - This is analog power should needs to be high PSSR (>65dB over wide frequency range)
        - `NCP163ASN280T1G`
            - Best option
            - https://www.digikey.com/en/products/detail/onsemi/NCP163ASN280T1G/10064750
        - `LP5907MFX-2.8`
            - Enable Pin: Low (default pulldown) turns off the regulator.
            - 1uF input and output caps.
            - https://www.digikey.com/en/products/detail/texas-instruments/LP5907MFX-2-8-NOPB/3906436
            - https://www.ti.com/lit/ds/symlink/lp5907.pdf
            - SOT-23-5 (SC-74A, SOT-753)
    - 1.8V : ~13mA peak
        - `TLV74318PDBVR`
            - https://www.digikey.com/en/products/detail/texas-instruments/TLV74318PDBVR/7593922
            - https://www.ti.com/lit/ds/symlink/tlv743p.pdf?HQS=dis-dk-null-digikeymode-dsf-pf-null-wwe&ts=1771252761776&ref_url=https%253A%252F%252Fwww.ti.com%252Fgeneral%252Fdocs%252Fsuppproductinfo.tsp%253FdistId%253D10%2526gotoUrl%253Dhttps%253A%252F%252Fwww.ti.com%252Flit%252Fgpn%252Ftlv743p
            - SOT-23-5 (SC-74A, SOT-753)
        - `TCR2EF18,LM(CT`
    - 1.2V : 155mA typical, 250mA peak
            - TLV75512PDYDR
                - This has an exposed pad.
            - https://www.ti.com/lit/ds/symlink/tlv755p.pdf?ts=1711517340379&ref_url=https%253A%252F%252Fwww.ti.com.cn%252Fproduct%252Fcn%252FTLV755P
