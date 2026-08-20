# Optical Motion Capture : Camera Hardware Design

Read this page if you want to know about all the hardware components of the mocap cameras (this is low level stuff for building or developing with the cameras).

## Overview

The camera electronics are divided into the following boards found in:

- [//pkg/vision/mocap/boards/compute](/pkg/vision/mocap/boards/compute/index.md) : Compute Carrier Board
- [//pkg/vision/mocap/boards/led](/pkg/vision/mocap/boards/led/index.md) : LED Ring Board
- [//pkg/media/camera/boards/camera_ar0234](/pkg/media/camera/boards/camera_ar0234/index.md) : AR0234 Camera Board
- [//pkg/media/camera/boards/camera_ffc](/pkg/media/camera/boards/camera_ffc) : 22-pin Flat Flex Cable 

3D Printed and CNC design files are in the [../parts](../parts) folder.

In addition to the above components, make sure to read about:

- [Camera sensor selection](./camera_sensors.md)
- [Lens selection](./camera_lens.md)

## Mechanical Assembly

**General dimensions**

- CM5 PCB is 1.2mm thick
- CM5 sits 0.5mm above the carrier board
- The CM5 passive heatsink can have a max of 3.5mm of screw inserted
    - Recommended minimum screw insertion is ~1.35mm (M2.5's 0.45 pitch x 3) to get a few threads of grip
- The main compute/led PCB boards are 80mm tall by 48mm wide
- All camera boards are 32 x 32 mm with 28 x 28 mm M2 hole spacing.

**Sandwich spacing**

- Space between Compute and Camera Board: 4mm
- Standoff Height: 17mm
- LED Heatsink Height
    - (around screw holes): 1.6mm
    - (max): 12mm
- LED Heatsink Washer Height: 0.4mm
- PCB thicknesses: 1.6mm
- Lens TTL: ~22.5mm
    - This is the distance from the image sensor to the farthest tip of the Lens when focused.
    - The board spacing and LED positioning are tuned to work well for roughly this value (+/- 1mm).

**Board Spacing** (between compute and LED boards):

- Exactly 23mm from top of compute board to bottom of the LED board
- Male 0.1" header has ~2.5mm of insulation.
    - Male header insertion distance is ~6.5mm into the female header
- Female 0.1" header has 8.5mm of insulation
- Bridging using this extension header:
    - https://www.digikey.com/en/products/detail/samtec-inc/SSQ-108-03-G-S/1111553
    - Cheaper tin plated ones: https://www.digikey.com/en/products/detail/samtec-inc/SSQ-108-03-F-S/6692119
    - DO NOT TRY BUYING ON ALIEXPRESS / AMAZON. The cheap ones are typically thinner and don't fit well.

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
- Provide V_POE and GND via the header
    - Any DC voltage >= 9V is fine here
- Verify 5V output
- Disconnect the test probes
- Plug in regulated PoE input
- Verify 5V output and limited current draw
    - Note: Use a constant current source since PoE will turn itself off without some minimal load applied.
- Flash over USB

TODO: Document LED board testing

TODO: Document flashing and testing the camera boards.

## 3D Printing Parts

All 3d printed parts should be made of matte black ASA and scaled to be dimensionally accurate (typically scale X/Y by 100.5%).

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

