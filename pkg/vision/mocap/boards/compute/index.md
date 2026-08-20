# Optical Motion Capture : Compute Board

This directory contains the design files for the compute board that does all the processing for the mocap camera.

## Components

### Compute Module

For doing marker recognition in camera images, we recommend using a Raspberry Pi CM5 (or CM4). Any of them should work but the recommended specs are:

- no-Wifi
- 1GB (CM4) or 2GB (CM5) (as low as it can go)
- 16GB EMMC (or a lite version with external SDCard)

Specific Models:

- What I currently use: CM5002016 / SC1558 (cheapest CM5 with eMMC)
    - Cheapest CM5 Lite (no eMMC): CM5002000 / SC1556
- Cheapest CM4: CM4001000

**Compute Modules Comparison**

Basically any CM4/CM5 compatible boards (with a camera input and ethernet) can be attached to the carrier board, but they vary a bit in capabilities which will impact how well it works.

For accurate time sync, we need good Ethernet PTP support on the compute module board:

- S-tier (hardware PTP PPS in Ethernet MAC)
    - Pi CM4 / Pi CM5
- A-tier (software PTP in Ethernet PHY)
    - Radxa CM3

MIPI bandwidth also varies which will impact future upgradability to higher FPS / resolution sensors in the future (note that the AR0234 can run at full speed on all modules listed below)

- S-tier (4 x 2.5Gbps MIPI lanes) 
    - RK3566 / RK3588 boards
- A-tier (4 x 1.5Gbps MIPI lanes) 
    - Pi CM5
- B-tier (4 x 1Gbps MIPI lanes)
    - Pi CM4

Performance is less critical since we use pipelining tricks to process the image and for most compute modules, the processing time is faster than it takes to receive the frames. Though it is still good for safety headroom and tail latency to process things faster:

- S-tier
    - Pi CM5
- A-tier
    - Pi CM4
- B-tier 
    - RK3566 boards

What is more important is having enough RAM bandwidth to allow a few processing passes over the images (DDR3+ speeds are a must).

**Carrier Board Connector**

(need 2x)

- [DF40C-100DS-0.4V(51)](https://www.digikey.com/en/products/detail/hirose-electric-co-ltd/DF40C-100DS-0-4V-51/1969495)
    - This is the standard part used on the offical CM5 IO board 
    - Cheaper on [LCSC](https://www.lcsc.com/product-detail/C597931.html)
- Overall the CM5 PCB will be 1.5mm above the carrier board PCB
    - Note that there are components on the bottom of the CM5 so there is basically zero clearance for components on the carrier board below the CM5.
    - There is a taller version of the connector but we don't use it.

**Camera Connector**

We aim for the board to be Raspberry Pi camera compatible:

- The standard CM5/RP5 camera connector is 0.5mm pitch with 22 pins
    - Exposes all 4 MIPI lanes
    - (what we use)
- The old CSI connector (used on most camera boards) is 15-pin 1.0mm pitch
    - Exposes only 2 MIPI lanes
    - Buy an adapter cable like this:
        - https://www.pishop.us/product/raspberry-pi-zero-mini-camera-cable-38mm/?srsltid=AfmBOoqGjUmR2GsKmsP6ZGImINLrKVcBaJtso3WF6fwT979VAPVBhP9t
        - https://www.aliexpress.us/item/3256807463693256.html

Camera GPIO Usage:

- GPIO0 : Used as camera 'enable' input to camera
- GPIO1 : Used as external trigger input to camera

The connector we are using on the camera and carrier boards:

- [FH34SRJ-22S-0.5SH(50)](https://www.digikey.com/en/products/detail/hirose-electric-co-ltd/FH34SRJ-22S-0-5SH-50/5132526)
    - (best 0.5mm 22p connector)
    - This is a slightly cheaper option which still has locking and has contacts on both top and bottom
    - Even cheaper on [LCSC](https://www.lcsc.com/product-detail/C2889325.html)

**Heatsink**

Recommend getting the standard Pi CM5 passive heatsink (or [Edatec heatsink](https://www.digikey.com/en/products/detail/edatec/ED-CM4COOLER-B/16683008) for CM4).

Note: We do NOT recommend using a fan since that will introduce extra vibrations.

**SDCard Slot**

The SDCard slot on the compute board is only supported when using a lite compute module version (without eMMC). The compatible parts for the slot connector are:

- https://www.digikey.com/en/products/detail/amphenol-cs-fci/10067099-200LF/4238739
- https://www.digikey.com/en/products/detail/molex/0475710001/3262277

### Power Input

Each camera module will get power over ethernet (PoE+). PoE+ network switches deliver between 42.5V - 57V DC to each device and support up to 25.5W of usable power on each ethernet device (note that not all network switches allow all ports to hit this limit simultaneously).

Expected power consumption:

- Pi CM5 (assuming 100% CPU usage)
    - 10W (Peak)
- Camera sensor
    - 1W (Peak)
- LEDs
    - 5-10W depending on exposure time

TODO: Add a current monitor to the PoE input.

### PoE Circuitry

Note that we are not isolating the PoE power so the camera will be more efficient but most be sufficiently enclosed so that a user can't touch any power lines.

- Magjack ethernet connector (1Gb/s with PoE+ support)
    - TMJG4926HENL or LPJG0926HENL4R
        - Cheapest to but on LCSC: https://www.lcsc.com/product-detail/C22457393.html
        - `LPJG0926DNL` is without LEDs but is only more economical at high volume. 
    - This is the same part as used on the Raspberry Pis.
- Data Lines TVS Diode
    - 2 x [TPD4EUSB30](https://www.lcsc.com/product-detail/C90627.html)
        - This is the standard part used by the CM5 IO board.
- 2 x rectifiers to extract 48V DC power
    - Ideally 100V max reverse voltage for safety
    - [CD-HD201](https://www.digikey.com/en/products/detail/bourns-inc/cd-hd201/6561443)
    - Slightly cheaper borderline safety option [CD-HD01](https://www.digikey.com/en/products/detail/bourns-inc/CD-HD01/6561441)
- PoE Controller
    - [TI TPS2378](https://www.digikey.com/en/products/detail/texas-instruments/TPS2378DDAR/3431161)
        - [Datasheet](https://www.ti.com/general/docs/suppproductinfo.tsp?distId=10&gotoUrl=https%3A%2F%2Fwww.ti.com%2Flit%2Fgpn%2Ftps2378)
        - Startup inrush current limit is 140mA
        - We have ~75ms to fully charge all capacitors during in-rush (else we need more circuitry to current limit)
        - So ~100-120uF is the max bulk capacitance we can have.
        - Need to wire up `CDB` to followup DC-DC converters to ensure they don't turn on until in-rush phase is done.
        - `CDB` is low when in inrush current mode and high-z the rest of the time.
        - Tie `APD` to low to disable external power adapter input.
    - Components for the controller
        - `C_1 = 0.1 μF, 100 V, 10% ceramic capacitor`
        - `D_1` (TVS diode). `SMAJ58A` recommended
        - `R_DEN = 24.9 kΩ ± 1%`
        - `R_CLS = 63.4 ohm`

### 5V Buck Converter

This as a converter from the PoE DC voltage to 5V 3A (mainly used by the CM5 CPU):

- [LM65645RZTR](https://www.digikey.com/en/products/detail/texas-instruments/lm65645rztr/26812452)
    - [Datasheet](https://www.ti.com/lit/ds/symlink/lm65645.pdf?ts=1754925441963&ref_url=https%253A%252F%252Fwww.ti.com%252Fpower-management%252Facdc-dcdc-converters%252Fproducts.html)
    - LM65635 can be slightly cheaper to get and also works (the only difference is that the 5V over current limit is 3.5A instead of 4.5A)

- Inductor
    - Recommended part: https://www.digikey.com/en/products/detail/codaca/SPRH1210-150M/21190892
        - Orientation shown in https://www.codacainductor.com/sprh1210
    - Recommended Part: https://www.digikey.com/en/products/detail/pulse-electronics/PA4320-153NLT/6555163
        - Very cheap in asia: https://www.lcsc.com/product-detail/C2453652.html
        - (I can't find any documentation on the orientation of this part so not very EMC safe)
    - Premium Option: https://www.digikey.com/en/products/detail/coilcraft/mss1210-153med/21381203

### PPS Divider

The carrier board has an STM32 based MCU that acts like a fancy software defined PLL that takes as input the PPS signal from the Raspberry Pi and divides it into 2 pulsed signals (camera trigger and LED strobe trigger) at the camera's FPS. Each of these signals has an independently controllable pulse width an time offset.

**Why this is needed?**

- The Broadcom PHY Driver on the CM4/5 only supports 1 PPS
    - This is fixable but in my experience the driver is also buggy and I have had to fix multiple race conditions (especially since on the CM5 the communication latency is higher due to the RP1) so it is risky to depend on this code for a high frequency signal.
- The Broadcom PHY Hardware on the CM4/5 only supports short pulse widths
    - Need a wider width for some sensors (e.g. Mira220) that use the pulse width as the exposure time
    - Need a wider width for the LED strobe signal
        - Many cameras have a STROBE output for this but this would require running an additional separate wire (we don't have any more pins on the 22 pin connector) to the camera and dealing with varying support for this in camera drivers.
- We often need to offset the STROBE output relative to the TRIGGER output (to avoid capturing images before the LEDs are completely on).
    - Broadcom PHY only exposes 1 output
    - STROBE output signals support this on camera sensors but that requires running an extra pin (see above argument).
- A convenient opportunity to add a TCXO to the board.
    - The crystal I am currently using on the PPS divider is 2.5ppm which is likely over 10x more accurate than the Broadcom PHY's one.
    - In the future, we can use this to 'discipline' the Broadcom PHY's crystal to speed up convergence time of PTP and minimize un-corrected drift over time.

**Parts**
- [STM32G031G8U6](https://www.digikey.com/en/products/detail/stmicroelectronics/STM32G031G8U6/10300275)
    - Cheaper option once in stock: [STM32G031G4U6](https://www.digikey.com/en/products/detail/stmicroelectronics/STM32G031G4U6/10326694)
- [ASTX-H11-24.000MHZ-T](https://www.digikey.com/en/products/detail/abracon-llc/ASTX-H11-24-000MHZ-T/3641101?s=N4IgTCBcDaIIIGUAqANAtACQIxbWALAHQAMpAshgFppIgC6AvkA)
    - Cheaper drop-in: [ECS-TXO-3225MV-240-TR](https://www.digikey.com/en/products/detail/ecs-inc/ECS-TXO-3225MV-240-TR/10478746)
    - Or [ATX-H11-F-24-000MHZ-F25-T](https://www.digikey.com/en/products/detail/abracon-llc/ATX-H11-F-24-000MHZ-F25-T/16634925)


### Accelerometer

Chip: [LIS2DW12TR](https://www.digikey.com/en/products/detail/stmicroelectronics/LIS2DW12TR/7348326680) (LIS2DH12TR is also supported)

The accelerometer is used mainly for initializing camera orientation estimates. It is not strictly necessary but simplifies alignment.

[Datasheet](https://www.st.com/resource/en/datasheet/lis2dw12.pdf)

- Up is -Y (same as pixel space)
- Right is -X (same as pixel space)
- Towards Camera is Z

Testing:

```
i2cdetect 1
=> Will use /dev/i2c-1
=> Should see address 0x19 respond
```


### Optional Carrier Board Features

The 'compute' module carrier board has a few extra features on the PCB that are not strictly required so can be left unpopulated if you want to save cost:

- SDCard Slot (recommended)
    - Symbols: `J9`, `C14`, `R12`, `U9`
    - Can be excluded if you intend on only using compute modules with embedded eMMC.
- RGB / IR Strobe Support (recommended)
    - Symbols: `R7`, `R11`, `R14`, `D2`, `D3`, `R13`, `R3`, `Q2`
    - These are needed if you want to connect the 'led' board for night vision or tracking passive markers
- PoE Voltage Monitor (recommended)
    - Symbols: `R4`, `R8`, `C9`
- Ethernet LEDs (recommended)
    - Symbols: `R9`, `R10`
    - The LEDs can be turned off in software so it is recommended to have them for early debugging.
- Accelerometer (recommended)
    - Symbols: `U4`, `C11`, `C12`
    - This is a quality of life chip to make calibration slightly easier and adds some extra features like automatically flipping the camera view and detecting camera shifts.
- Lens Filter Switcher Driver
    - Symbols: `U10`, `C25`, `C26`, `F1`, `J5` 
- All the other 2 pin Molex Picoblade connectors
    - You only need these if you want to support custom camera boards.
    - These are easy to solder in by hand later on.

## Manufacturing Specifications

### PCB

Should use FR4, 1oz outer / 0.5oz inner copper weight, 4 layer, ENIG plating.

There are several impedance controlled traces on the board (all non-coplanar) so you also need to make sure that you specify a specific stackup with your PCB manufacturer and make sure that the traces in Kicad have the current width and gap. To modify the spacings:

- Use the online impedance calculator available with your manufacturer to find the 100 ohm, 90 ohm, 50 ohm trace sizes (see example results below).
- In the Kicad board editor:
- Click `File` -> `Board Setup...` -> `Net Classes`
- Add new `Netclasses` with your manufacturer's `_MANUFACTER_NAME` suffix.
    - These are predefined for JLCPCB and NextPCB
- Update the `Netclass Assignments` to point to these.
- Hit Ok to exit out of this.
- Click `Edit` -> `Edit Track and Via Properties`
    - Filter items by one of the net classes
    - Use `Set to net class / custom rule values`
    - Hit `Apply`
    - Repeat for all the netclasses.
- Save and export the gerber files for production.

Recommended Stackup (`>= R4` board revision) (0.5oz inner):
- JLCPCB
    - JLC04161H-3313
        - 100 Ohm (differential pair) (Net Class: `DP_100_JLC`)
            - DP Width: 0.1143 mm
            - DP Gap: 0.2 mm
        - 90 Ohm (differential pair) (Net Class: `DP_90_JLC`)
            - DP Width: 0.1460 mm
            - DP Gap: 0.2mm
        - 50 Ohm (single ended) (Net Class: `SE_100_JLC`)
            - Width: 0.1425 mm
- NextPCB
    - 04161H01-2116
        - 100 Ohm (differential pair) (Net Class: `DP_100_NEXT`)
            - DP Width: 0.1491 mm
            - DP Gap: 0.1651 mm
        - 90 Ohm (differential pair) (Net Class: `DP_90_NEXT`)
            - DP Width: 0.1757 mm
            - DP Gap: 0.1384 mm
        - 50 Ohm (single ended) (Net Class: `SE_100_NEXT`)
            - Width: 0.20828 mm

Recommended Stackup (`<= R3` board revisions) (1oz inner):
- JLCPCB
    - JLC041611-7628
        - 100 Ohm
            - DP Width: 0.1732 mm
            - DP Gap: 0.15mm
        - 90 Ohm
            - DP Width: 0.2337 mm
            - GP Gap: 0.15 mm
- NextPCB
    - 04161H03-7628
        - 100 Ohm
            - DP Width: 0.18542 mm
            - DP Spacing: 0.137922 mm
        - 90 Ohm
            - DP Width: 0.24511 mm
            - DP Spacing: 0.138684 mm

### Stencils

Per-board recommend getting 2 140x140mm stencils (one for each side of the PCB).

Get them 'electropolished' since some of the pitches are very small.

### Soldering

The recommended way to build the board yourself is as follows:

- Solder bottom side (with compute module connectors)
    - Solder Paste: `GC10 SAC305T4`
    - DO NOT add attach the inductor (or add solder paste for it). It has very high thermal mass so will make reflowing the other side much harder to do consistently without a long soak time.
- Solder top side
    - Solder Paste: `NC191LT250`
- Attach the inductor
    - Use low temp solder in a synringe like `Chip Quik SMDLTLFP`
    - Hot air to melt it.
    - Note: Pin #1 on the inductor should be facing away from the board edge (on the side like the via grid)
- Add all the through hole connections desired.
- Brush/bath with isopropyl alcohol to clean flux residue.

Important tips:

- DO NOT use old (dry / somewhat dry) solder paste. Given the components on this board, this will end up leading to a lot of bridging and bad electrical performance.


## Change Log 

- `R2` (well supported)
    - First stable
    - Only supports the Pi CM5 with eMMC
    - Compute module pins
        - GPIO2: Accelerometer I2C SDA
        - GPIO3: Accelerometer I2C SCL
        - GPIO4: MCU_UART_RX
        - GPIO5: MCU_UART_TX
        - GPIO10: RGB_SERIAL (SPI MOSI)
        - GPIO12: STROBE_DIMMING (PWM)
        - GPIO16: FRAME_TRIGGER
            - Must use PIO to forward to CAM_GPIO1
        - GPIO17: MCU_SWDIO
        - GPIO20: SOLENOID_DIR2
        - GPIO20: SOLENOID_DIR1
        - GPIO22: CAM_GPIO1 (Just used for PIO)
        - GPIO26: MCU_NRST
        - GPIO27: MCU_SWCLK
        - ETH_SYNC_OUT : Goes to the MCU
        - CAM_GPIO0 routed to camera
        - CAM_GPIO1 routed to camera
- `R3`
    - Adds ethernet LED resistors
- `R4`
    - Adds SDCard slot
    - Supports CM4/5
    - Requires some modifications for CM4 support (do all or none)
        - Solder the jumper and drill out the CAM_GPIO1 connection near the compute module connector.
        - Do not use PIO for pin forwarding
- `R5`
    - Proper CM4 support
- `R6`
    - Compatible with more boards (but introduces some pinout changes):
        - Supports Pi CM4 / Pi CM5
    - Probably compatible but needs testing:    
        - Radxa CM3 / Orange Pi CM4 / Pine64 SOQuartz
    - Probably 'mostly' compatible
        - Banana Pi BPI-CM4 : Does not support setting the RGB LED colors.
    - Fan no longer supported
    - Compute Module Pinout diff
        - GPIO14: MCU_UART_RX (GPIO4 now disconnected)
        - GPIO15: MCU_UART_TX (GPIO5 now disconnected)
        - GPIO12: Disconnected (moved to MCU)
        - GPIO16: Disconnected
        - GPIO19: ETH_SYNC_OUT : This is an extra output that is connected to the regular ETH_SYNC_OUT. To be used on none Raspberry Pi boards that don't have a native ETH_SYNC_OUT
        - CAM_GPIO1: Removed since it can't be used on CM4
        - Fan no longer supported
    - MCU Pinout diff
        - PA8: STROBE_DIMMING (PWM) : Moved from the compute module
            - The latest MCU firmware will unconditionally generate this signal since it is disconnected on old board revisions anyway.
- `R7`
    - Fixing vias under chips
- `R8`
    - Greatly improving high voltage protection on the interface with the LED board
    - Quality of life improvements to component placement.
    - MCU Pinout Diff
        - PA7 (SPI1_MOSI) : Now attached to the RGB LED serial input
        - PA6 (ADC1_IN6) : New V_POE_SENSE pin (old one disconnected)
    - Compute Module Pinout Diff
        - Removed RGB_SERIAL
- `R9`
    - Improving buck converter power planes and current path through inductor (much lower voltage ripple)
- `R10`
    - Switching buck convert to FPWM (fixed frequency mode) : Not as good efficiency but much better voltage ripple.
    - Adding some more filtering caps.
