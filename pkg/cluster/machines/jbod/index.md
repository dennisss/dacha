# JBOD Enclosure

TLDR: [Watch this video first](https://www.youtube.com/watch?v=vVI7atoAeoo)

This project is a custom built SAS JBOD enclosure meant to store 45 drives in a 4U 19" server rack slot.

Other requirements is that it must fit in a short 550mm long rack (excluding the front cover) and must be fairly silent. Both of these are achieved by only using high efficiency consumer grade SFF PSUs and large 120mm Noctua fans.

This enclosure will only contain management, SAS expander, and power electronics and will need to connect to another computer via external SAS cables to actually read/write from the disks.

## Known Quirks

- The PSUs can be just barely removed for replacement without taking everything apart, but its a bit annoying to unplug the wires.
    - You may want to clip off the retaining clips on the power connectors so that they are only friction fit.
- You need to add a 40mm or larger fan behind the expanders to keep them cool.
    - They haven't failed on me, but will likely have a short life without extra cooling.
    - Haven't yet designed a proper mount, so for now improvising with some glue.
- Cover may shake a bit under high random read/writes due to insufficient clamping on the rear side.

## Parts

This section contains information on all the individual parts we are using and why they were selected. Use this as a general guide for buying compatible parts for new builds. 

### Sheet Metal Parts

There are 5 pieces that need to be made of sheet metal:

- `Body`
- `Rear Fan`
- `Back`
- `Cover`
- `Front`

The designs are currently configured to be used with the following metals (these are available from `Send Cut Send`):

- G90 Steel : 0.075in thick
    - Use for `Front`

- G90 Steel : 0.059in thick
    - Use for `Body`, `Rear Fan`, `Back`
    - K-Factor: 0.36
    - Bend Radius: 0.063
    - Bend Deducation: 0.1117"
    - Bend Allowance: 0.1323 in

- G90 Steel : 0.036"
    - Use for `Cover`
    - K-Factor: 0.38
    - Bend Radius: 0.045in
    - Bend Deducation: 0.073in
    - Bend Allowance: 0.0922

Bending directions:

- `Body`
    - All up 90 degrees
- `Rear Fan`
    - All down 90 degrees
- `Back`
    - All down 90 degrees
- `Cover`
    - All up 90 degrees
- `Front`
    - N/A

You should get the following hardware pre-installed in the sheet metal parts:

- `Body`
    - For mounting rails:
        - 6 x `Flush Nut, M5 x 0.8`
        - Pull Direction: Down
        - Insert into some of the 4.0mm holes
    - For attaching the front panel
        - 7 x `Nut, M4x0.7`
        - Pull Direction: Down
        - Insert into some of the 4.0mm holes
    - For attaching the rear fan and back panels
        - 3 x `Flush stud, M4 x 0.7, .315"`
        - Pull Direction: Up
        - Insert into the 4.11mm holes
    - For installing the backplane standoffs
        - Recommend getting M3 flush studs. Any length between 2 - 6mm (prefer around 3mm).
        - You will need 48 of these.
        - The alternative is to use low profile M3 screws.
- `Rear Fan`
    - 4 x `Nut, M4x0.7`
    - Pull direction: Up
    - These all go into the side holes to fix this panel to the body frame.
- `Back`
    - 4 x `Nut, M4x0.7`
    - Pull Direction: Up
    - Insert into the 4.4mm diameter wholes on the side (all but the bottom hole)
- `Cover`
    - None
- `Front`
    - None

### 3D Printed Parts

All the parts are designed to be printed roughly dimensionally accurate and I usually use ASA filament with 100.5% scaling for most of them. Important settings for specific parts are listed below:

- The main two disk retainer pieces
    - Print in 40%+ infill.
    - Do everything you can to prevent the parts from warping (adding brims, etc.).

- Printing Fan Spacers and Washers
    - TPU 95a
    - Infill: 10% triangles
    - 2 top and bottom solid layers
    - 1 perimeter
    - Rest default recommended for for the printer (probably will have high extrusion multiplier or high parameter/infill overlap to get solid layers).

- 18mm Standoffs
    - Print with default Prusa PETG settings (0.45mm extrusion width, 0.4mm nozzle) and no scaling
    - Should shrink to 8mm outer diameter / 4mm hole diameter
        - Standard size short 3mm heatset inserts should snap.

### Drives

We aim to support 45 of the `WD HC550 18TB SAS` drives in the enclosure though you can use any similar drives:

- Full Name: `Western Digital Ultrastar DC HC550 WUH721818AL5204 0F38353`
- Peak Current
    - 5V:   1A
    - 12V:  2A
- Average (Active) Current
    - 5V:   0.5A
    - 12V:  0.6A
- Connector
    - 29pin SAS SFF 8680

### Power Supply

Since peak current requirements are fairly large, we will use 2 x `Corsair SF600/500 PSUs`. Each PSU will be wired up to ~half of the drives with no fallback to the second PSU. There is not much point in going to a higher wattage SFF PSU (e.g. SF1000) since the 5V power envelope doesn't increase so is still too low from a single PSU to handle even average active power draw.

Note that the PSUs need to be short SFF style (100mm) to be able to fit in the case.

Specs for a single PSU:

- Max Current
    - 5V: 20A
    - 12V : 50A
- Female Connectors (on the PSU)
    - General rule of thumb is that each pin is limited to 8A
        - So assuming each given each backplane blade has up to 4 drives (peak 4A@5V, 8A@12V), we want one dedicated pin going to each blade for 12V and 0.5 pins per blade for 5V.
        - So overall need 6 12V+GND pins and 3 5V+GND pins per PSU.
    - One extra of each of the 12V, 5V, and GND pins is also required to run to the management board.

**Cabling to get:**

- Male PSU connector set
    - https://www.moddiy.com/products/Modular-Connector-Full-Set-7pcs-for-Corsair-SF.html
    - Also need a crimping tool (I use a `ENGINEER PA-21` for all crimps in this build).
- 18 AWG wire
    - ~10 meters of black (GND)
    - 5 meters of white (12V)
        - (or yellow if you want to follow PC standards)
    - 5 meters of blue (5V)
        - (or red if you want to follow PC standards)

### SAS Expanders

We will use 2 x `Adaptec AEC-82885T Expander`s (easier to buy on eBay). Specs from each of them:

- 1 (or 2) x Mini SAS HD => 6 x Mini SAS HD
- Power requirement: 1.34A @ 12V
- Supports `SES-3` for querying the processor temperature.

Each expander will be powered from a single PSU.

### Fans

- 6 x `NF-F12x25`
    - Each uses peak 0.14A @ 12V
    - So total is 0.84A
- By default fan power is pulled from the left power supply but this will fallback to the right one if ther left one is now powered on. This switching is done via a relay to avoid electrically coupling the two 12V lines.


### Fasteners

- If I forgot to mention a screw type, then it is probably one of these:
    - M3 x 6mm Button Head screws
        - IDK how many. A pack of 100 screws is enough for the entire build
    - M4 x 6mm Button Head screws
        - IDK how many. A pack of 100 screws is enough for the entire build
- Screwing Front Panel to Body
    - 7 x M4 6mm pan head
- Screwing fans on
    - 24 x M4 35mm pan head screws
    - 24 x M4 standard hex nuts
- For attaching rear fan and back panels to the body frame
    - 3 x M4 nuts
    - 3 x M4 washers
- For attaching the disk retainer to the frame
    - 8 x `M3 x 5.7mm` heatset inserts
    - 8 x M3 6mm button head screws
- For attaching the backplanes to the standoffs:
    - 48 x `M3 x 6mm` button head screws
    - 96 x M3 x 3mm short heatset inserts in all the 3d printed 18mm standoffs
- For attaching SAS expanders to the management holder
    - 3 x `M4 x 4mm` heatset inserts
    - 3 x `M4 24mm` screws
- For attaching the management board to the management holder
    - 2 x `M2 x 3 x 3.5mm` heatset inserts
    - 2 x M2 4mm machine screws
- For attaching LED strips to the disk retainer
    - 24 x `M2 x 3 x 3.5mm` heatset inserts
    - 24 x 3d printed M2 washers
    - 24 x M2 6mm machine screws
- For connecting the two halfs of the disk retainer
    - 4 x 3mm diameter 16mm long steel dowels
    - CA / super glue
- For connecting the flaps to the disk retainer
    - 8 x 3mm diameter 8mm long steel dowels
    - CA / super glue
- Magnets for the management holder
    - 4 x 6mm diameter 2mm tall 

Note that I got all the M3 and M4 heatset inserts from CNC Kitchen.


### Backplane

The backplane is composed of individual blades that connect 3 or 4 SAS drives to 1 Mini SAS HD connector going to the SAS expander. In total the enclosure will 3 rows of blades with each row having 1 x 3-drive blade and 3 x 4-drive blades. This is in contrast to enclosures like the HL15 which uses a single PCB for an entire row: the single PCB approach makes alignment easier but is more annoying to solder and more expensive if replacing an individual component.

The latest stable board design is in the `boards/backplane-r2` folder.

Pre-exported production files are located here (download both as `.zip` files):

- https://storage.googleapis.com/da-sources/sha256/997430bd59564e8fb671e5ee1241b263c594608e8afde709ecd4b5834af568be
- https://storage.googleapis.com/da-sources/sha256/973f699f585c04ecb686cd4979749b07df52c551a60728020d31dc800b225064

You need to order at least 3 of the 3-disk version and 9 of the 4-disk version.

The exact PCB stackup you order matters such that the board is designed for controlled 100 ohm differntial impedance and high power (8A). The settings that currently work are the following from JLCPCB:

- 1oz outer and 1oz inner copper. 4 layer. 1.6mm pcb
- 'JLC041611-7628' stackup.
    - We use the following values in the design to make the SAS data lines 100 ohm impedance:
        - 0.2mm trace spacing
        - 0.2126mm trace width
- Min via hole size: 0.3mm
- Min trace width/spacing: 0.1mm (4 mil)
- 135*135mm stencil


Mini SAS Screws

- M2 x 0.4mm self threading screw
    - Length: PCB Thickness + 2.5mm max

Other components:

- Power Connector
    - 2x2 Micro-Fit
        - https://www.digikey.com/en/products/detail/molex/0430450423/3044577
    - Mating Male housing
        - https://www.digikey.com/en/products/detail/molex/0430250400/252497
    - Crimp Terminal
        - https://www.digikey.com/en/products/detail/molex/0430300040/11503719?s=N4IgTCBcDaICwGYAMylLkkBdAvkA

- SAS Drive Connector
    - https://www.digikey.com/en/products/detail/molex/0878390018/5116557

- SAS Data Connector
    - SFF-8643 (Mini SAS HD)
    - https://www.digikey.com/en/products/detail/molex/0768671011/4693322
    - https://www.digikey.com/en/products/detail/te-connectivity-amp-connectors/2227580-1/5445073
    - https://www.digikey.com/en/products/detail/amphenol-cs-commercial-products/G40H11331HR/5775380

Note that the SAS/SFF parts are standardized and there are multiple manufacturers that make effectively identical parts.

### PCBs

The following PCBs are needed to build the JBOD:

- `boards/management`: (1x) This is the main "motherboard" that controls the power supplies, fans, etc.
    - 2 layer PCB. Use any standard 1.6mm PCB process.
- `boards/led`: (12x) These are the LED strips to illuminate the disks.
    - These are 1-2 layer PCBs.
    - The bottom layer is only used for strengthening so you can make a 1 layer board though 2 is preferred.
- `boards/led_bridge` (4x) These are the boards to bridge the edge connectors betweend the LED boards
    - 1 layer 0.8mm PCB recommended.

For testing the boards, you can also make the following test boards:

- `boards/backplane-tester` : This tests individual backplanes
    - 2 layer PCB
- `boards/power-tester` : This tests backplane cables (you can do this once you are able to turn up our PSUs).
    - 1 layer PCB


## Software

This section describes all the host and microcontroller software required for getting this thing working.

Note that all the MCUs used in this project are currently nRF52 modules running custom firmware that will require a suitable bootloader flashed to work. See [this page](/pkg/peripherals/doc/flashing.md).

### Testing

The power and backplane tester boards use a generic firmware that can be flashed as follows:

```
# Build the firmware
cargo run --bin builder --  build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840

# Flash it
cargo run --bin flasher built/pkg/nordic/nordic_radio_dongle uf2-dfu
```

Then these are the commands to run the testing (assuming the boards are connected to the current computer via USB):

```
cargo run --bin jbod_tester -- test-backplane --log_path=backplane_data.csv --board_id=xx

# TODO: Change the multimeter_addr to the IP address is a SCPI capable multi-meter.
cargo run --bin jbod_tester -- test-power --multimeter_addr=10.1.0.135
```

### Management

The management board uses the same firmware but has a custom config to give it a distinct USB device id:

```
cargo run --bin builder --  build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840_jbod_management

cargo run --bin flasher built/pkg/nordic/nordic_radio_dongle uf2-dfu
```

The software to control the management board from a computer is located in [//pkg/cluster/jbod/index.md
](/pkg/cluster/jbod/index.md).
