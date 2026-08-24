# Optical Motion Capture : Camera Hardware Design

Read this page if you want to know about all the hardware components of the mocap cameras (this is low level stuff for building or developing with the cameras).

## Overview

The camera electronics are divided into the following boards found in:

- [//pkg/vision/mocap/boards/compute](/pkg/vision/mocap/boards/compute/index.md) : Compute Carrier Board
- [//pkg/vision/mocap/boards/led](/pkg/vision/mocap/boards/led/index.md) : LED Ring Board
- [//pkg/media/camera/boards/camera_ar0234](/pkg/media/camera/boards/camera_ar0234/index.md) : AR0234 Camera Board
- [//pkg/media/camera/boards/camera_ffc](/pkg/media/camera/boards/camera_ffc) : 22-pin Flat Flex Cable 

3D Printed and CNC design files are in the [../parts](../parts) folder. Follow the 3d printing guidance [located here](#3d-printing)

In addition to the above components, make sure to read about:

- [Camera sensor selection](./camera_sensors.md) (optional if you just use the AR0234 board)
- [Lens selection](./camera_lens.md)

## Mechanical Assembly

**General dimensions**

- CM5 PCB is 1.2mm thick
- CM5 sits 0.5mm above the carrier board
- The CM5 passive heatsink can have a max of 3.5mm of screw inserted
    - Recommended minimum screw insertion is ~1.35mm (M2.5's 0.45 pitch x 3) to get a few threads of grip
- The main compute/led PCB boards are 80mm tall by 48mm wide
- All camera boards are 32 x 32 mm with 28 x 28 mm M2 hole spacing.

**Board Spacing** (between compute and LED boards):

Between the compute and LED boards there is 1 male header, 1 female header, and a bridging male-female header:

- Exactly 23mm from top of compute board to bottom of the LED board
- Male 0.1" header has ~2.5mm of insulation.
    - Male header insertion distance is ~6.5mm into the female header
- Female 0.1" header has 8.5mm of insulation
- Bridging using this extension header:
    - The one we are using has 8.5mm of insulation (female part) and a 10mm male part (of which 6.5mm plugs into the other female header).
    - https://www.digikey.com/en/products/detail/samtec-inc/SSQ-108-03-G-S/1111553
    - Cheaper tin plated ones: https://www.digikey.com/en/products/detail/samtec-inc/SSQ-108-03-F-S/6692119
    - DO NOT TRY BUYING ON ALIEXPRESS / AMAZON. The cheap ones are typically thinner and don't fit well.

If you add up all the dimensions (`2.5 + 2*8.5 + (10 - 6.5)`), you'll find that there is **23mm between the boards**.

**Sandwich spacing (R1)**

- Space between Compute and Camera Board: 4mm (use R1 camera mount)
- Standoff Height: 17mm
- LED Heatsink Height
    - (around screw holes): 1.6mm
    - (max): 12mm
- LED Heatsink Washer Height: 0.4mm
- PCB thicknesses: 1.6mm
- [Lens TTL](./camera_lens.md): ~22mm + ~0.15mm

**Sandwich spacing (R2)**

- Space between Compute and Camera Board: 4.4mm (use R2 camera mount)
- Standoff Height: 17mm
- LED Heatsink Height
    - (around screw holes): 1.6mm
- NO WASHERS UNDER HEATSINK (use thermal paste to directly attach heatsink)

**Lens to LED Spacing**

Note that the camera spacing relative to the LED ring is very important:

- The M12 lens must stick up past the LED board so that it can be easily turned for focused.
- If it isn't far enough above the LEDs, the LEDs will shine into the lens at an extreme angle and cause haloing.
- If it is too high, the LEDs will be clocked by the lens.

TODO: Update this section for the R2 LED heatsink.

## Testing

This is testing that can be done on the electronics boards before we setup the final software

Testing the compute board:

- First assemble the board without CM5 / camera / LEDs attached.
- Use a multimeter to verify GND / V_POE / 5V are not connected.
- 5V Basic Functionality
    - Provide V_POE and GND via the header
        - Any DC voltage >= 9V is fine here
        - Limit current draw to 0.5A
    - Verify 5V output
- 5V Limit Test
    - Boost input power to 50V with 1A current limit
    - Use a constant current source to verify you can pull 3.5A of 5V power for more than 10 seconds without any dropout.
- Disconnect the test probes
- Plug in a regulated PoE input
- Verify 5V output with 100mA current draw
    - Note: Use a constant current source since PoE will turn itself off without some minimal load applied.

TODO: Document LED board testing

## [3D Printing Parts](#3d-printing)

All 3d printed parts should be made of matte black ASA and scaled to be dimensionally accurate (typically scale X/Y by 100.5%). Generally use 3 perimeters, 0.45mm extrusion width, 15% infill.

Case notes:

- You will need a 2mm drill bit to bore out the screw holes on the case-bottom piece.
- The 1/4-20 tripod holes are sized at 4.8mm. These need to be drilled out to 5.1mm and tapped (either by hand or by machine)
- The top and bottom halves of the case are designed with 0.6mm of spacing so when you screw them together over the PCBs, they will squeeze together and the PCBs should NOT be loose inside of the case.

## Fasteners

- Attaching the camera to the 3d printed camera mount
    - 4 x M2 x 4mm
    - 4 x M2 3mm height, 3.5mm diameter heatset inserts (3.2mm narrow side diameter)
- PCB Sandwich Screws
    - 4 x M2.5 32mm (30mm barely works too)
- Exterior Case
    - 4 x M2 20mm
    - 4 x M2 3mm height, 3.5mm diameter heatset inserts (3.2mm narrow side diameter)

## Change Log

**Case**

- `R1`
    - Origin design
    - Compatible with 12 RGB LEDs
- `R2`
    - Adds a slot for the SD Card present on newer computer boards.
- `R3`
    - Changes to support the 2 RGB LED boards.
    - The top cover is now taller and acts to block very wide angle rays from entering the lens.
    - Acts 1/4-20 mounting holes (tapped)
    - Width/height is also a big bigger to round to an exact # of millimeters.

**Camera Mount**

- `R1`
    - Original design
    - Height is 4mm
- `R2`
    - Increased height to 4.4mm
    - This is to be used with the LED heatsink directly attached to the LED board with thermal paste (no washers)
    - The camera is pushed up slightly to reduce the risk of LED glare into the lens for some lenses that aren't fully shrouded.

