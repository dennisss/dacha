# Voron 0 Build 

TLDR: Watch the videos to get the most detailed guidance (https://www.youtube.com/playlist?list=PLhpElgVsLSPczMurblBVNrXqp7tCt836-) and come to this page for specific details.

This is a log describing my Voron 0.2r1 derived 3d printer build. Most of the chosen parts don't follow any specific kit and are instead self sourced with the goal of optimizing for part quality/precision, print speed, and reliability for up to 60 degree celsius chamber temperatures. A further aim is to ensure that as many components as possible can be monitored for failures (e.g. broken fans, motors, heaters).

The core Voron build is based on the version saved to the submodule in [//third_party/voron_0/base](//third_party/voron_0/base/). You should generally be following the [main assembly guide](//third_party/voron_0/base/Manuals/VORON_V0.2r1_Assembly_Manual.pdf) while altering the steps per the advice on this page.

The most major mod we apply on top of this is the [Tulip (//third_party/voron_0/tulip)](//third_party/voron_0/tulip/) mod which adds double shear, live idlers, and dowel pins for idlers. If you watched the videos, I printed the lazy cam versions, but they ended up not fitting with all the other parts, so prefer to use the regular versions.

## Components

This section describes specific components we chose for this build which are worthy of mentioning as well as the mods we apply on top of the base Voron 0 BOM.

### General Part Selection

- All fasteners are 18-8 passivated stainless steel from McMaster-Carr.
- For panels we use:
    - ACM material for all opaque panels to improve thermal insulation.
    - Polycarbonate (PC) for all transparent panels since it has a higher thermal resistance than Acrylic.
    - I bought panels from [here](https://www.tensor3d.com/products/copy-of-voron-panel-kits-voron-v0-2?variant=41784090296365).
    - DO NOT BUY 1/8in or 3.175mm panels as they probably won't fit. Only get 3mm advertised thickness panels.
- Motors are the same as the LDO kit ones.
- Genuine Bondtech gears are used for the extruder.
- Most electronics parts are running at 5V with motors at 24V.

### Extrusions

I got clear Makerbeam XL beams and drilled per the Voron 0 stock and Tulip instructions.

- The Makerbeam XL extrusions are already threaded on the ends so you only neeed to drill out side holes.
- Holes need to be at least 3mm diameter 
- I did all my drilling with a Makera Carvera and the gcode files located [here](./extrusions/) (read the top of each file to got a summary of what you need to do)

### Printing

Unless otherwise mentioned, 3d printed parts are made using the standard Voron settings. THese are what I used:

- Filament:
    - Main Color: Polymaker Polylite ASA White
    - Accent Color: Polymaker Polylite ASA Pop Blue
- Prusa XL 0.4mm nozzle, 0.2mm layer height ASA profile with changes:
    - 40% Gyroid infill
    - 4 walls
    - 5 solid top/bottom layers
    - 0.4mm forced extrusion width
- Printed in ~50-60 deg C chamber.

### Hotend

- [Revo HeaterCore 24V / 60W / 104-NT](https://e3d-online.com/products/revo-heatercore?variant=41245338697787)
    - Peak power is ~65W (~2.7A)
    - Goal to support up to to 100W (~4.2A)
    - Standard connectors are `Molex Micro-Fit 3.0, 2 pin`
- Thermistor: `Semitec 104NT-4-R025H42G`
- The standard E3D sock is replaced with a [full coverage sock](https://levendigs.com/products/silicone-sock-x-for-revo-nozzle) to reduce cooldown from part cooling fans.
    - DIY [option 1](https://www.printables.com/model/152444-diy-all-around-silicone-sock-for-e3d-revo), [option 2](https://www.printables.com/model/942746-full-coverage-sock-mold-for-e3d-revo)

### Extruder

This is basically the stock Voron 0 extruder. Notes on extruder gearing math below.

Extruder Gearing: 

- Extruder Motor (LDO-36sth20-1004ahg)
    - Max 1 amp per phase
    - 1.8 degree per full step
    - 10 teeth gear
- Couples to a 50 teeth bondtech gear
    - So 5:1 reduction
- Bondtech drive gears
    - https://kb-3d.com/store/bondtech/91-bondtech-drive-gear-kit-175-5mm-7350011410187.html
    - ~7.45mm diameter from the rotation shaft to touching the filament
    - So circumerference is ~23.4mm (amount filament per drive gear rotation)

### Heated Bed

We use a Provok3d Voron Magbed which comes with a Mica heater with a resistance of 50 Ohms (290W on 120V AC). This let's it heat up close to 3x faster than the LDO 100W heater which seems to be the best heater available in any V0 kits. This is controlled via an Omron AC relay (Omron G3NA-210B).

- Thermistor is a PT1000 cartridge
- Build surface is a Cool X plate from West 3D.

The bed we are using is a sandwich of the following components:

- Cool X Plate
    - 66 grams
- Provok3d Bed Kit:
    - Backing Plate
        - 72 grams
        - Thickness: ~1.95mm
    - Mika Heater
        - 49 grams
    - Main Bed Block
        - 394 grams
        - ATP-5 with SmCo grade 26 magnets
        - Thickness: 9.52mm
        - Magnet holes: 36 holes each 6mm diameter x ?? Depth (at least 5mm)
    - Screws
        - 4 x M3 8mm socket head screws
        - Overall 3 grams

### Fans

We chose exclusively 3-pin (power, ground, tachometer) PWM controllable fans since having a built in tachometer is the most reliable way to tell if a fan is functional. All fans connected to the toolboard are 5V. The rest are mixed voltage.

- BFB0305HHA-CF00 (2 x Part cooling fans)
    - FAN BLOWER 30X10MM 5VDC WIRE
    - Peak 0.35A
    - 8500 RPM +/- 20%
    - 4 poles (2 pulses per revolution)
    - Recommended 8.2kOhm pullup and 4nF filter.
    - 1.7 CFM
    - 30 AWG wire
        - Insulation diameter is 0.7mm

- MR3010H05B1-RSR (1 x Hotend cooling fan, 1 x bed cooling fan)
    - FAN AXIAL 30X10MM BALL 5VDC WIRE
    - Current 0.18A
    - 10,000 RPM
    - 6.4 CFM
    - Recommendations are here: https://www.mechatronics.com/engineering/dc-fans-blowers/

- BFB0524H-R00 (Air filter fan)
    - FAN BLOWER 51.3X15MM 24VDC WIRE



## Electronics

### Power Supply

The recommended power supply is the [RPS-200-24-C](https://www.digikey.com/en/products/detail/mean-well-usa-inc/RPS-200-24-C/7706041). This is more compact than the stock power supply recomendation and doesn't need to be very high wattage since the bed is directly powered off of AC.

### Wiring

We recommend using FEP or PTFE insulated stranded copper wiring where possible and especially for the moving wires that go up to the bed.

The following set of wire gauges are used:

- (18AWG)[https://www.digikey.com/en/products/detail/cnc-tech/1330-18-1-0500-001-1-S/26737605] : High power + AC wiring
- 22AWG : For 5V power to the Raspberry Pi and LED strips.
- (24AWG)[https://www.digikey.com/en/products/detail/cnc-tech/10064-24-1-0500-001-1-TS/28220559] : General purpose / low power
    - Also use for the wires going to the bed board from the back compartment.
- (28AWG)[https://www.digikey.com/en/products/detail/cnc-tech/10064-28-1-0500-001-1-TS/4486265] : For the JST SH connectors on the toolhead and bed boards

Wire Colors (used in the videos):

- Black: 0V DC, Heatbed Wires, AC Hot
- White: +5V/+24V DC, AC Neutral
- Blue: Data line, AC Ground

Use a 4A fuse for the AC plug.

Bottom Wiring (this is a mapping from source to destination (indented) pin):

- AC Hot/Live (all 18AWG)
    - Wago 221 Connector (3-pin) 
        - 24V PSU
        - Heatbed SSR
- AC Neutral (all 18 AWG)
    - Wago 221 Connector (3-pin)
        - 24V PSU
        - Heatbed (Raw)
- AC Earth/Ground (all 18 AWG)
    - Wago 221 Connector
        - Screw into the frame
        - Heatbed (Raw)
        - 24V PSU body

- 24V Output (6-pin : 3 24V + 3 GND) (all 18 AWG)
    - 4 wires (2 24V + 2 GND) to the umbilical board
    - Other two wires go to 2 x 3 pin Wago 221 connectors 
        - Pair to 5V PSU
        - Pair to main board

For things like fans, motors, and heaters, it is ok to use the wires that come with the components and just trim and re-crimp them as needed:

- The stock E3D heater wire is ~20-22 AWG with teflon sleeving.

### Tool Board

To control the toolhead, a custom electronics board is available [here](./boards/toolhead/). There are no specific requirements for manufacturing aside from using a standard 4-layer PCB process.

Features:

- XT30 2+2 Host USB + 24V power connector
    - (you can find the connector on Aliexpress)
- TMC2209 for Extruder motor driving
- 1 x Hotend Heater Output
    - Up to at least 100W of continous draw
    - With current sensing.
- 1 x Hotend thermistor input.
- 1 x Hall effect analog input for filament sensing.
- 3 x PWM + Tachometer 5V Fan Inputs/Outputs
- 2 x Serial LED Outputs
- 1 x Extruder motor input.
- E3D PZ Probe input for bed leveling
- User programable button and RGB LED
- 'Chamber' temperature using nRF52 built in thermistor.
- Accelerometer (LIS2DH12TR)
- Magnetic encoder for extruder motor sending
    - Pair with a 1/16" thick by 1/4" diameter magnet

Flashing of the initial bootloader to the board can be done via the TC2030 connector. See [this page](/pkg/peripherals/doc/flashing.md).

Additional parts you will need:

- 10x10x10mm aluminum heatsink
    - Attach behind the motor driver with an adhesive thermal pad and secure with some silicone glue.
- Stainless steel 20mm M3 standoffs (4.5mm hex)
    - I buy these from McMaster Carr in place of the 3d printed standoffs in the stock instructions.

### Umbilical Board

The umbilical board sits between the AB motors and combines power+USB connections into the umbilical cable going to the tool board. You can find the CAD for the board [here](./boards/umbilical/).

To make the cable, you want to use cables that are rated for many flexes over time at the 3d printer's chamber temperature.

- Recommended cable is the Igus `CFBUS.065` model (comes with all 4 wires in one)
    - Cut off a 240mm piece of cable.
- Print 4 of the [xt30-cable-relief.stl](./toolboard/xt30-cable-relief.stl) parts and hotglue them around the cable ends after soldering on the connectors.

### Bed Board

The bed board is designed to attach to the CNC Bed mod and actions as an expansion board that allows you to connect many peripherals to your bed and just run a single set of power and serial wires back to your main boards.

Note that all components on the board itself can handle over 100C temperature though individual peripherals connected to the board will likely be limited below that.

Features:

- Serial + 5V input
- 1 x PWM fan+tachometer input
- 2 x temperature sensor (PT1000) inputs
- 1 x WS2812 style LED chain output
- Onboard chamber temperature sensing via the thermistor in the MCU

#### r1

The [revision 1 board](./boards/bed/r1/) is based on an ATTiny chip. It is very cheap to make but is not recommended because the ADC in the ATTinys is not very good and requires being calibrated (0 voltage offset and voltage scaling factor) to use reliably. Additionally due to the lack of pins, the serial and fan outputs are multiplexed so glitching of the fan may occur when changing the LED colors.

The firmware is located [here](./firmware/bed/r1/) and can be flashed via the Arduino UI with megaTinyCore installed.

Note: This board revision uses 5V serial input.

#### r2

The [revision 2+ boards](./boards/bed/) are based on an STM32C0 chip. The ADC is much higher quality than the R1 board so doesn't require any additional calibration.

This board can operate either in half-duplex (single wire serial) or duplex (UART TX + RX wire) mode. When operating in half-duplex mode, just route the TX wire back to your main board or Raspberry Pi and connect it to the RX pin on your host. Then attach a 1kOhm resistor between the TX and RX pins on your host.

The firmware is located [here](./firmware/bed/r2/)

Note: This board revision uses 3.3V serial voltage levels (this is in addition to the 5V power input).

### Main Board

The main control board used to drive the motors is located [here](./boards/main/).

### RGBW Sequin LEDs

The recommended LEDs to use for the CNC bed mount and the toolhead are the [Pinlight](https://github.com/dracotonisamond/Voron-Stuff/tree/main/Pinlight) boards with [IN-PI33QBTPRPGPBPW](https://www.inolux-corp.com/datasheet/SMDLED/Addressable%20LED/IN-PI33QBTPRPGPBPW-XX_v1.0.pdf) LEDs.

## Mods

### CNC Bed

A custom CNC machined aluminum bed frame is used which replaces the extrusions or Kirigami bed braces in the Voron instructions. The weight of the bed is around the same as the Kirigami bed but offers substantially better (>5x in simulations) rigidity. Since it is unlikely to not be flat, we also abandoned using thumb screws and instead use 8mm stainless steel standoffs to mount the heated bed to the bed frame (stainless steel is chosen due to low thermal conductivity while also being resistant to the full bed temperature range).

Note that because the bed frame is taller than the standard options, we use longer 165mm linear rails for Z to avoid reducing the print volume.

Notes for printing the parts:

- These should all be printed to accurate dimensions after shrinkage (I printed at 100.5% XY scale to achieve this).
- The `z-nut-bracket.stl` should be printed with the large z-nut hole flat on the build plate.

Fasteners:

- Attaching the bed to the linear rails
    - 8 x M2 4mm socket head screws
    - DON'T FORGET TO USE THREAD LOCK
- Attaching the wago holder to the CNC bed
    - 3 x M3 12mm button head screws
    - The bed Wago connectors (that also come with the kirigami kit) are Wago 221-412
- Attaching the bed board to the CNC bed
    - 4 x M3 2mm plastic spacer
    - 3 x M3 8mm button head screws
    - 1 x M3 10mm button head screw
        - Longer one to attach a grounding wire (match sure it is electrically connected to the aluminum)
    - Note that in R4+ of the bed, these screws are intended to thread in from the bottom through the bed board into the aluminum bed.
- Attaching the Z-nut bracket to the aluminum bed
    - 4 x M3 6mm button head screws
    - 4 x M3 Voron style heatset inserts
- Attaching the cable chain clip to the aluminum bed
    - 2 x M3 6mm button head screws
    - 2 x M3 Voron style heatset inserts
    - Add another 2 heatset inserts for attaching to the cable chain.
- Attaching the floor cover to the aluminum extrusions
    - 2 x M3 6mm button head screws
- Attaching the LED holder to the bed
    - 2 x M3 6mm button head screws
    - 2 x M3 3mm short standard heatset inserts (not the voron type, but the CNC kitchen sized ones)
- Attaching a fan to the aluminum bed
    - 4 x M3 12mm screws for the fan
    - Make sure that air is blowing up (towards the heated bed) to avoid hot air overheating the fan.

Fasteners for the heated bed (bottom to top):

- 3 x M3 4mm screws going up into the standoffs
- 3 x M3 4.5mm hex; 8mm height stainless steel standoff
    - Both sides of the standoffs should be threaded to accept M3 screws. 
    - Maybe include some washers if the bed doesn't end up level.
- 3 x M3 10mm button head screws going down into the standoffs (through the heated bed aluminum block)


I also add a backstop to the beated bed plate (these are basically tabs to prevent you from sliding the PEI sheet back further than it should be):

- 2 7.8mm OD M3 washer
- 2 M3 x 5/6mm low profile screws for the washers
- 2 3mm ID / 4mm OD / 3mm height spacer for the washers
- Only use all-metal parts


### Toolhead Magnetic Filament Presence Sensor

This is a mag that allows for sensing whether or not filament is inserted into the toolhead. There are mainly 3 new/modified components:

- [mag-filament-guidler-solid.stl](./mods/mag_filament/mag-filament-guidler-solid.stl) : Use in place of the stock guidler piece and glue a 6x3mm magnet into the hole
- [mag-filament-motorplate.stl](./mods/mag_filament/mag-filament-motorplate.stl) : Use in place of the back toolhead plate.
- Insert a hall sensor like a [SS49E](https://www.digikey.com/en/products/detail/honeywell-sensing-and-productivity-solutions/SS49E/701361) into the hole in the motor plate and wire it back to the toolboard.
    - Use with 30 AWG wiring and heat shrink.

When printing the guidler, the following settings are recommended:

- Filament: PCCF
- Nozzle Size: 0.6mm
- "External perimeters first": Enabled
- Extrusion Multiplier: 0.94
- Extrusion Width: 0.6 fixed
- Nozzle Size: 0.6mm
- Infill: 100%

### Wiper Servo

This is a compact servo actuated wiper for the nozzle to clean up filament build up.

Parts:

- Motor: TowerPro SG92R
- Silicone Wiper: "Heatbed Nozzle Wiper" for "A1 mini" from Bambu Labs
- 4 x M2 heatset inserts (3mm length)
    - 2 go into the body and 2 go into the wiper arm
- 2 x M2 4mm screws
    - For attaching the cover to the body
- 2 x M2 6mm screws
    - For attaching the silicone wiper to the arm
- 2 x M3 6mm button head screws
    - For attaching the body to the aluminum extrusion

Prework:

- Modify the motor by cutting off the stock sides with motor screw holes and sanding them down
- Extend the motor wires with 24 AWG wiring so that they can reach the back compartment of the printer.
- Press-fit the wiper arm to servo motor and fasten with one of the self-threading screws included with the motor.

### Raw Ethernet Jack Skirt

This is a an alternative rear skirt piece that press fits a raw ethernet jack. Get a right angle through hole (8P8C) RJ45 jack, solder wires directly to the picks, cut off the plastic support pins and secure with hot glue to prevent the wires from coming off.

I originally used the keystone based one referenced in the third party mods section but it ended up being bulky and I had a hard time getting my ethernet wire to properly crimp in keystone connectors.

### Electronics Tray

### Motor Encoder

### Duet3d Filament Sensor

The [duet_filament_sensor_mount.stl](./mods/duet_filament_sensor/duet_filament_sensor_mount.stl) piece allows you to attach a [Duet 3D filament sensor](https://www.duet3d.com/filamentmonitor) below the umbilical board. The output of the filament sensor will line up exactly with the hole in the umbilical board.

Note: This requires having 2 extra nuts pre-inserted into the aluminum extrusion next to the umbilical board.

Optionally you can also use this [simplied sensor board](../../boards/duet3d_alt_magnet/) which exposes the magnetic encoder via a simpler I2C QWIIC interface (the magnetic encoder chip is the same one used on the original Duet 3D board).

For future reference, these are the screws that come with the Duet 3D filament sensor:

- 2 x M2.5 12mm socket head
- 1 x M2.5 6mm socket head

### Dual Pane Panels

Extra polycarbonate inserts are provided in [inner-side-panel-bottom.dxf](./mods/dual_pane/inner-side-panel-bottom.dxf) that can be added in the middle of the side aluminum extrusions to provide additional insulation. These also provide holes to mount LED strips.

Note that there panels must be inserted EARLY in the build before the front aluminum extrusions are inserted.

### Front Camera Mount

The [front-camera-mount.stl](./mods/front_camera/front-camera-mount.stl) mounts a [3DO enclosure camera](https://github.com/3DO-EU/Enclosure-Nozzle-Camera-V2) in the front left of the printer on the front left vertical aluminum extrusion.

- 2 x M3 nuts must be pre-inserted into the front-left aluminum extrusion (at the same time as inserting the nuts for the door magnet holder).
- 2 x M3 6mm button head screws secure the 3d printed mount to the extrusion.
- Get the 25cm FPC extension cable for the camera
    - Currently this is the longest cable 3DO sells. The spacing is a bit tight as is and how high up you can mount the camera is limited by the cable length.

Route the cable through the channels made for the LED strips.

### LED Strips

This is a custom mod that allows mounting RGBW LED strips to the dual pane mods and provides cable routing for both the LED strips and the camera.

- LED strips used are the [RGBW Matchsticks from Provok3D](https://west3d.com/products/rgbw-bw-neo-match-stick-everything-on-a-stick-led-lightstick-for-3d-printers-5v)
- Print 2 x [matchstick-adapter.stl](./mods/matchstick-adapter/matchstick-adapter.stl) for the right side
- Print 2 x [matchstick-adapter-sliced-9mm.stl](./mods/matchstick-adapter/matchstick-adapter-sliced-9mm.stl) for the left side (this has the slot for routing the camera cable)
- Insert 8 x M3 short heatset inserts (CNC Kitchen style) into the adapters.
- Secure everything together using 8 x M3 6mm nylon screws
    - Replace 2 of these with M3 4 - 4.5mm screws (you can cut down 6mm ones) for mounting the left strip to the L shaped adapter clip to allow sliding the camera cable into the adapter

For cable routing, the mid panel (behind the Z axis linear rails), should be drilled per [mid-panel.dxf](./mods/panels/mid-panel.dxf). The larger hole is for the left side (with the camera cable). If you want to drill them out manually, you can cut out multiple side by side holes with a 3mm drill bit.

### Side Panels

Side panels with hexagonal air holes.

### Rear Panel Magnetic Hinge

This mod adds a magnetic hinges rear panel to the printer. Note that the stock rear panel is 212mm wide. This needs to be re-cut to 207mm wide.

For the right side, use this [hinge mod](https://www.printables.com/model/548771-voron-0-hinged-back-panel-w-hinged-filament-spool) (see also the Third Party Mods section).

For the left side, print:

- [rear-panel-magnets-top.stl](./mods/rear_panel_magnets/rear-panel-magnets-top.stl)
    - Print 2 of these (lay the flat side on the bed).
    - Midway through the print, insert a 6mm x 3mm N52 magnet into each piece (try to keep the orientations the same)
    - The slightly lower surface on the non-flat side of these parts is where the VHD tape goes to attach the piece to the rear panel.
- [rear-panel-magnets-bottom.stl](./mods/rear_panel_magnets/rear-panel-magnets-bottom.stl)
    - Print 2 of these
    - Insert 6mm x 3mm N52 magnets (make sure they are oriented to attract to the magnets in the top pieces).
    - These attach to the rear extrusion using 4 x M3 6mm button head screws


### Third Party Mods

Main third party mods used are:

- [Rear Panel Hinge](https://www.printables.com/model/548771-voron-0-hinged-back-panel-w-hinged-filament-spool)
    - Use 4 x M3 6mm screws to attach the hinges to the back aluminum panel
    - Use 2 x M3 40mm screws to attach the hinge halves
    - After printing, sand down the hinges until the two halves fit smoothly together
    - Also, you probably want to drill out the hinge pieces with a 3mm drill so that the hinge screws fit with only a little bit of  force.
- [Skirt Mesh](https://www.printables.com/model/369688-voron-02-v02-skirt-set-mesh-only)
    - These are the meshes only. Superglue to the base skirt pieces.
    - Part files mirrored in [//third_party/voron_0/skirt_mesh_only](//third_party/voron_0/skirt_mesh_only)
- [Skirt Mesh Front](https://www.printables.com/model/418525-headless-skirt-add-on-for-voron-02-v02-skirt-set-m)
    - Extra mesh for the front skirt piece (if not using a display).
    - Part files mirrored in [//third_party/voron_0/skirt_mesh_front](//third_party/voron_0/skirt_mesh_front)
- [MFNano Remix Carbon Filter](https://www.printables.com/model/500513-voron-v0-tiny-recirculating-carbon-filter-mfnano-r)
    - The stock parts didn't fit for me, so I am using custom parts in the [./mods/filter/](./mods/filter/) folder.
    - Stock part files mirrored in [//third_party/voron_0/filter](//third_party/voron_0/filter).
- [Matchstick Diffusers](https://github.com/VoronDesign/VoronUsers/tree/main/printer_mods/MapleLeafMakers/Matchstick_Diffusers)
    - Part files mirrored in [//third_party/voron_0/matchstick_diffusers](//third_party/voron_0/matchstick_diffusers).
    - Printed in PETG

Honorable mentions:

- [Deck Plate Cover](https://www.printables.com/model/405522-voron-v02-deck-plate-cover-for-kirigami-mode)
    - This is the inspiration for our CNC Bed deck cover.
- [Better Guidler](https://www.printables.com/model/848709-voron-v0-0-02-v02-improved-guidler)
    - Parts mirrored into [//third_party/voron_0/better_guidler](//third_party/voron_0/better_guidler)
    - We derive our filament sensor mod from this model as the base.
- [Rear Keystone Jack Skirt](https://www.printables.com/model/533549-voron-02r1-rear-skirt-wkeystone)
    - Part files mirrored in [//third_party/voron_0/skirt_rear_keystone](//third_party/voron_0/skirt_rear_keystone)
    - Note: An extra skirt mesh for this as available at [./mods/rear_keystone_skirt_mesh/](./mods/rear_keystone_skirt_mesh/)
    - I didn't end up sticking with this and switched to a custom ethernet mod instead.
- [Midbody with PTFE Tube Coupler](https://www.printables.com/model/737826-voron-02-mini-stealthburner-ercf-push-fit-ecas-4mm)
    - This uses a [ECAS04](https://www.trianglelab.net/products/ddb-extruder-embedded-collet-clips?VariantsId=10592)
    - Part files mirrored in [//third_party/voron_0/modmidbody_ecas](//third_party/voron_0/midbody_ecas).
    - I would use this, but the mod needs to be revised since the hole is too small for the ECAS fitting.


## Software

The software for the printer is comprised of the following core components:

- MCU Firmware : Code in [//pkg/nordic/src/controller](/pkg/nordic/src/controller)
    - This is a pretty generic firmware that takes configuration requests from a host machine.
- Controller : Code in [//pkg/cnc/controller/](/pkg/cnc/controller/)
    - This is an RPC service that connects to all the MCUs and handles core logic like motion planning, heater, fan, and endstop control.
    - Note that this service is intentionally minimal and doesn't understand things like leveling, gcode, etc.
    - This is configured in [//pkg/cnc/controller/config/voron0.txtpb](/pkg/cnc/controller/config/voron0.txtpb)
- Other tools and UIs will communicate with the controller to tell the printer to do stuff.

### MCU Flashing

Assuming the bootloader is already flashed to the boards, you can flash the main board and toolboard as follows:

Plug in just the main board via USB to your computer:

```
cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840_voron0_main
cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:0001
```

Then plug in the toolhead board:

```
cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840_voron0_tool
cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:0001
```

If you also made the aux board that controls the LED strips:

```
cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840_voron0_aux
cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:0001
```

Note that this instals the same firmware to both boards but they are differentiated based on the usb device id (the bootloader has a `8888:0001` id, the main board will get a `8888:000a` device id and the toolhead will get a `8888:000b` device id)

### MCU Benchmarking

With a microcontroller attached via USB to your computer (and ideally nothing else attached to it), the following commands can be used to benchmark the maximum number of commands per second that can be sent and the minimum width of step pulses that the MCU can generate:

```
cargo run --bin cnc_tools --release -- benchmark noop --usb_device_id=8888:000a
cargo run --bin cnc_tools --release -- benchmark step-width --usb_device_id=8888:000a
```

### Bed Heater Calibration

For bed heater calibration, we do the following:

First, collect data by trying to drive the bed at various power levels:

```
cargo run --bin cnc_controller -- --mode=measure-bed --log_path=data/bed/measurements.csv
```

Then we will solve for a physics model of how the bed works:

```
cargo run --bin cnc_controller --release -- train-bed-model \
    --log_path=data/bed/measurements.csv \
    --step_output_dir=data/bed/training_steps \
    --weights_output_path=data/bed/training_weights.csv
```

Example of controlling the bed using the physics model:

```
cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    [--step_output_dir=data/bed/control_steps \]
    [--results_path=data/bed/control_results.csv]
```

### Hotend Heater Calibration

To calibrate the toolhead, we currently require having an SCPI capable multimeter attached to the local network which has a thermocouple attached which is inserted into the nozzle for measuring the 'true' nozzle temperature.

Then run a command like the following to do data collection while heating the nozzle at different rates:

```
cargo run --bin cnc_controller --release -- measure-toolhead \
    --log_path=data/toolhead.csv \
    --multimeter_addr=10.1.0.134
```

Using this data, we can estimate a physics model that matches the nozzle as follows:

```
cargo run --bin cnc_controller --release -- train-toolhead-heater-curve \
    --log_path=data/toolhead.csv 
```

### Starting the controller

You can run the following command to start the controller on your local machine:

```
cargo run --bin cnc_controller -- service \
    --config_name=voron0 \
    --port=8000
```

If you have setup a cluster using this [guide](/pkg/cluster/index.md), you can start it as a persistent service on your printer's Raspberry Pi as follows:

```
cargo run --bin cluster_cli -- \
    start_job pkg/cnc/controller/config/voron0.job
```

### Basic CLI Commands

The `cnc_tools` binary can be used to send a variety of commands to the controller. Some examples below:

```
# Perform XYZ homing
cargo run --bin cnc_tools -- execute --proto="commands: [{ home {} }]"

# Clear alarm state (entered if we hit an unexpected endstop or there is a code error)
cargo run --bin cnc_tools -- execute --proto="
    commands: [
        { reset_alarm: true }
    ]
"

# Change target extruder temperature.
cargo run --bin cnc_tools -- execute --proto="commands: [{ set_temp { target: 215 } }]"
cargo run --bin cnc_tools -- execute --proto="commands: [{ set_temp { target: 0 } }]"

# Move bed down 10mm
cargo run --bin cnc_tools -- execute --rel_z=10

# Extrude 50mm of filament
cargo run --bin cnc_tools -- execute --extrude=50

# Change part cooling fan speed to 100% (1.0)
cargo run --bin cnc_tools -- execute --proto="commands: [{ set_fan_speed { speed: 1 } }]"

# Move to specific locations in a sequence
cargo run --bin cnc_controller -- execute --proto="
    commands: [
        { move_to { x: 80 y: 40 z: 10 feed_rate: 20 } },
        { move_to { x: 40 y: 40 z: 10 feed_rate: 20 } },
        { move_to { x: 40 y: 80 z: 10 feed_rate: 20 } },
        { move_to { x: 80 y: 80 z: 10 feed_rate: 20 } }
    ]
"
```

### Skew Calibration

We can perform skew calibration of the printer using a camera rigidly attached to the toolhead and a calibration board with known dimensions.

**Camera Mount**: Parts to mount a [3DO Enclosure Camera V2](https://github.com/3DO-EU/Enclosure-Nozzle-Camera-V2) to the toolhead are in the [./mods/toolhead_3do_camera/](./mods/toolhead_3do_camera/) directory.

**Calibration Pattern**

I recommend using a 7x5 inch (179mm x 128mm to be exact) glass picture frame.

Generate a calibration pattern by running:

```
python3 pkg/cnc/scripts/create_charuco_pattern.py

# Convert to PDF
# See https://stackoverflow.com/questions/52998331/imagemagick-security-policy-pdf-blocking-conversion if this errors out.
convert -density 600 charuco_board.png charuco_board.pdf
```

Print out the pattern at 100% scale on a printer with quality settings set to 'high' if available. Measure that the box grid is the right size (each box is supposed to be 5mm wide so the overall grid size should be an exact multiple of 5mm). If it is incorrect, adjust the `SCALE_X` and `SCALE_Y` parameters in `create_charuco_pattern.py` and regenerate the pattern.

Attach the pattern to the glass with spray adhesive while trying to keep the pattern flat.

**Running**

Then assuming the camera is attached to your computer via USB, run the following to run a grid scanning of the pattern. This will run multiple grid scans of the bed where in each round, the pattern should be oriented in a different angle/direction on the bed.

```
cargo run --bin cnc_tools -- skew-calibration scan
```

I will now have a bunch of images of the pattern, so run the following script to recognize the pattern in the images and dump the camera positions:

```
python3 pkg/cnc/scripts/recognize_charuco_pattern.py
```

Finally, calculate the overall skew matrix and save it to a file:

```
cargo run --bin cnc_tools -- skew-calibration calculate --output_path=skew.txtpb
```

If you want to make the images into a video, you can use the following commands:

```
cargo run --bin cnc_tools -- skew-calibration dump-video
ffmpeg -f concat -safe 0 -i skew_video_stamps.txt -vsync vfr -pix_fmt yuv420p skew_camera_video.mp4
```

### Leveling

To perform mesh bed leveling, just running the following command and the mesh data will be saved to a file:

```
cargo run --bin cnc_tools -- leveling mesh-level --output_path=mesh.txtpb
```

Other commands for debugging:

```
# Directly connects to the toolhead board (bypassing the controller) and prints out the probe readings.
cargo run --bin cnc_tools -- leveling test-probe

# Runs 100 rounds of Z probing in the center of the bed.
cargo run --bin cnc_tools -- leveling probe-variance

cargo run --bin cnc_tools -- leveling dump-mesh --input_path=mesh.txtpb
```

### Running a Print

Before running a gcode file, make sure you have already:
- Homed the printer
- Heated up the extruder
- Run bed leveling

Then you can run the following command to simulate running a gcode file and estimate the overall time required: 

```
cargo run --bin cnc_tools --release -- execute \
    --gcode_file=testdata/cnc/voron0/voron0-benchy-fast.gcode \
    --z_leveler=mesh.txtpb \
    --skew=skew.txtpb \
    --simulate
```

Re-run without `--simulate` if there are no errors during simulation.

