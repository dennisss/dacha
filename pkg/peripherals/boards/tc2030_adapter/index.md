# TC2030 Adapter

This is an adapter for converting between a 2x5 0.05" pitch pin header (from a Black Magic Probe) to a 2x3 0.1" pitch pin header which can either be used by:

- Connecting to a [TC2030-IDC-NL](https://www.tag-connect.com/product/tc2030-idc-nl) cable which adapts this to a target PCB.
- Directly running jumper wires from the 0.1" header to the programming points on your target PCB.
    - Minimally you need to run GND, SWDIO, SWDCLK, and VREF (should be the power voltage of the target MCU so probably connect to 3.3V).

The pin of the adapter connectors looks like this:

![adapter pinout](images/adapter.jpg)

## Build Guide

General Steps:

- Make the PCB located in the `board` directory (these are the KiCAD design files)
- 3d print the parts in the `parts` direction
- Hot glue the case pieces together
    - Note that the buttons are just press fit and don't need glue.

For the PCB, it is ideal to make the proper PCB but you can also just use some prototyping board and a SWD breakout board like [this](https://www.adafruit.com/product/2743) to make it quickly. This is what is looked like when I first make it:

![proto board front](images/proto_board_front.jpg)

![proto board back](images/proto_board_back.jpg)