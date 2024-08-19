# Carvera

The [Carvera](https://www.makera.com/products/carvera) is a desktop CNC milling machine machine sold by Makera. 

## Board

The main control board can be found behind the back panel of the machine. The board is enclosed in the below box:

![](./images/board-cover.jpg)

Under the cover is the main PCB that looks like below:

![](./images/board.jpg)

Technical details of the board:

- Main Processor: LPC1768FBD100
- Co-processors
    - ESP M8266 module for WiFI
    - CC2530 module (currently unused?)
- Micro SD card slot (labeld `TF Card`) wired directly to the main processor
- USB-C female poart (labeled `USB-UART`)
    - Normally routed to the machine back panel USB-C port
    - On the board, linked to an FTDI 232RL USB-to-Serial IC and then to the main processor
    - FTDI chip supports up to a 3MHz baud rate. (divides a 48Mhz reference clock)  
- X,Y,Z axes
    - Main processor connected to external motor drivers via 2 x 74HCT245 logic transceiver ICs
- A,T axes
    - 2 x TMC2209 stepper motor drivers onboard.

## Firmware

The main processor runs Smoothieware with custom patches. Source code is [here](https://github.com/brooklikeme/Smoothieware/tree/makera). Smoothieware documentation can be found [here](https://smoothieware.github.io/Webif-pack/documentation/web/html/index.html).

TODO: Maintain a fork repository.

### Compiling

I'd recommend making the following batch to the firmware source before compiling:

```
diff --git a/src/libs/Kernel.cpp b/src/libs/Kernel.cpp
index 5b2c9072..82d85946 100644
--- a/src/libs/Kernel.cpp
+++ b/src/libs/Kernel.cpp
@@ -127,7 +127,7 @@ Kernel::Kernel()
     // default
     if(this->serial == NULL) {
         // this->serial = new(AHB0) SerialConsole(P2_8, P2_9, this->config->value(uart_checksum, baud_rate_setting_checksum)->by_default(DEFAULT_SERIAL_BAUD_RATE)->as_number());
-       this->serial = new(AHB0) SerialConsole(P2_8, P2_9, 115200);
+       this->serial = new(AHB0) SerialConsole(P2_8, P2_9, 1000000);
     }
 
     //some boards don't have leds.. TOO BAD!
```

Compiling the firmware can be done with:

```bash
# Do once
./linux_install

# Do on every compile
./BuildShell
DEFINES="-DNO_TOOLS_EXTRUDER " DEFAULT_SERIAL_BAUD_RATE=1000000 make clean all
```

Note that we pick 1MHz as a baud rate since both the LPC and FTDI chips have clocks that are even multiples of 1MHz.

Then the `LPC1768/main.bin` file can be copied to the Carvera's SDCard as `firmware.bin`.

Test the machine by sending `M114` via serial and it should show the position. Also disable WiFI by running `ap disable`.

### SD Card

The machine comes with a generic 16 GB sdcard formatted with one FAT32 partition named `CARVERA`. The root files are:

- `config.default`
- `config.txt` : Main config file for Smoothieware
- `FIRMWARE.CUR` : Current firmware flashed to the machine.
    - Firmware can be re-flashed by adding a `firmware.bin` file to the SDCard that will be renamed to this `.CUR` file name after successful flashing. 
- `gcodes/` : Directory storing gcode example files.


## Camera

![](./images/camera-inside.jpg)

There is a camera mount for a USB 1080p camera (see cameras [here](../../pkg/media/camera/index.md)) with a heatsink attached. It fits inside the front of the machine with the lid closed. 

Installing:

- Print:
    - For the ELP camera:
        - [parts/camera-top.stl](parts/camera-top.stl)
        - [parts/camera-back.stl](parts/camera-back.stl)
    - For the Arducam camera:
        - [parts/camera-top-arducam.stl](parts/camera-top-arducam.stl)
        - [parts/camera-back-arducam.stl](parts/camera-back-arducam.stl)
    - [parts/arm.stl](parts/arm.stl)
    - 2 x [cable-holder.stl](parts/cable-holder.stl)
    - [parts/wall-hinge.stl](parts/wall-hinge.stl)
    - [parts/floor-bracket.stl](parts/floor-bracket.stl)
- Enclosure the camera using the camera-top/bottom parts:
    - For ELP short
        - 4 x M2 14mm machine screws
    - For ArduCam
        - 4 x M2 18mm machine screws
    - Use with 4 x M2 hex nuts
- Attach the camera to the `arm.stl` and then attach the other side of the arm to the `wall-hinge` using:
    - 2 x M3 14mm machine screws
    - 2 x M4 hex nuts.
- Attach the wall hinge to a 200mm 1515 extrusion:
    - 2 x 5-6mm M3 screws
    - 2 x M3 hex nuts
- Connect the extrusion to the floor bracket
    - 1 x 6mm-7mm M3 machine screw
    - 1 x M3 hex nut
- For connecting the floor bracket to the carvera
    - The stock screw is M4 x 6mm (countersunk - length measured from tip to tip)
    - Replace with a M4 x 8mm machine screw
    - Additional hot glue can be used on the back side 
- The `cable-holder.stl` parts can be used for cable routing along the floor of the carvera.
    - The stock floor screws are M4 x 8mm (countersunk - length measured from tip to tip)
    - Re-use the stock screw (might be a tight fit) or replace with non-counter sunk 10mm screws


## Feeds and Speeds

https://wiki.makera.com/en/speeds-and-feeds

## GCode Reference

This section seeks to document what GCode sequences are needed to control the unique features of the machine.

- Makera documented GCodes are listed here: https://wiki.makera.com/en/supported-codes

- General Smoothieware supported Gcodes are documented here: https://smoothieware.github.io/Webif-pack/documentation/web/html/supported-g-codes.html

TODO: Mirror the above pages.

### Operations

#### Buffering

Lines can be rpefixed by `buffer ` to buffer the command for later execution. See the smoothieware `Player.cpp`.

#### Movement

- `M496.1` : Move to clearance position
    - Supposedly `G28` also works.
- `M496.2` : Move to work origin
- `M496.3` : Move to 'Anchor 1'
- `M496.4` : Move to 'Anchor 2'
- `M496.5 X??Y??` : Move to 'Path Origin'.
    - TODO: What are the parameters.

#### Leveling

- `G32 R1 X0 Y0 A10 B10 H2` : Grid probing
    - `X/Y` are the start position?? Supposedly these are relative to the current position
- `M495 X?? Y?? [C?? Y??] [O?? F??] [A?? B?? I?? J?? H??] [P1]` : Scan margin, then do z probe, change probe tool
    - `X` and `Y` are the min coordinates of the probing area
    - `C` and `D` are the X/Y max coordinates of the margin area to scan
    - `O` and `F` are the X/Y offset at which to conduct a Z-probe (relative to 'X', 'Y').
        - Internally this uses the `G38.2` command.
        - Note that there is also an 'absolute' probe mode if only `O0` is included
    - `A, B, J, H`
        - `A/B` are the width/length of the grid
            - 
        - `I/J` are the grid size
            - Default is 3,3
        - `H` is the height
            - Default is 5
        - Internally this uses the `G32` command
    - `P1` : If included, we will go to the origin after everything else is done. (origin is the X/Y coordinates given)
- `M495.3 [DXX] [HYY]` : Perfects an 'XYZ probe' using the manual tool setter.
    - `XX` : Tool diameter. Defaults to `3.175`
    - `YY` : Probe height. Defaults to `9.0`
    - Internally uses standard `G38` style commands.
- `M370` : Clear auto bed leveling data set by `M32`
- `M375.1` : Display bed leveling data.


#### Tool Changer

Code for switching to tool 1 is shown below. Valid tools indices are in the range `[0, 6]`. `-1` is also valid and means no tool. T0 is the wireless probe.

```
T1 M6
```

Other commands:

- `M6T-1`: Drop the current tool
- `M491`: Calibrate the current tool's length.
- `M493.2T0`: Set the current tool to tool index 0. All `[-1, 6]` values are valid.
- `M497.X` : Sets the ATC state to `X` (search for `ATC_NONE` in the source code to find the state enum)

TODO: Figure out how to 

#### Wireless Probe

To pair the wireless probe, start by running `M471`.

Press the wireless probe manually for 10 seconds (or until the green LED starts to blink).

If wireless pairing succeeds, the green LED will blink 5 times slowly. In either case, the green LED will switch off at the end of pairing (times out after 30 seconds).

If successful, the machine will print `WP PAIR SUCCESS` over serial. Else it will print `WP PAIR TIMEOUT`:q

Other undocumented codes:

- `M470 SXX` : Sets the wireless probe address to `XX` which is a 16-bit integer
- `M471` : Enters pairing mode
- `M472` : Turns on the wireless probe laser
- `M881 SXX` : Changes the 2.4GHz channel to `XX` which is an 8-bit integer
- `M882` : Stops 2.4Gz transmission.

#### Other

Toggleable things:

- `M7` : Airflow on
- `M9` : Airflow off 
- `M105`: Read current spindle temperature
- `M331` : Auto-vacuum mode on (vacuum only when spindle is running)
- `M332` : Auto-vacuum mode off.
- `M494.0` | `M494.1`  : Probe laser on?
- `M494.2` : Probe laser off
- `M801 S100` : Vacuum on (100%)
- `M802` : Vacuum off
- `M811 S100` : Spindle cooling fan on (100%)
- `M812` : Spindle cooling fan off
- `M821` : Light on
- `M822` : Light off
- `M831` : tool detector laser on
- `M832` : tool detector laser off
- `M841` : Wireless probe charging on
- `M842` : Wireless probe charging off
- `M851` : Extended power port on (Optional `S` parameter controls PWM output of the port)
- `M852` : Extended power port off

### Carvera Examples

This is GCode observed in the gcode found in the factory default sdcard.

Typical start gcode in a program:

```
G21 G40 G54           % mm mode, ??, use workspace coordinates
G80 G90 G94           % ??, absolute mode, ??
( Tool #2 "30degree0.2mm" / Diameter 3.175 mm )
T2 M06
M03 S12000            % Start the spindle.
M07                   % Start airflow
G00 X114.096 Y0.714
```

Typical end gcode:

```
G00 Z3
M09                  % Turn off airflow
M05                  % Stop spindle
M02
%
```

## Software

Carvera Controller

- Uses https://github.com/kivy/python-for-android
- All the source code in the APK is in `private.mp3` which is a tar file.

### Old

TODO: Re-verify all this information.

- `M999` : Reset from halted/alarm state.
- `$H` : Home all axes.
- `G21` : Set to millimeter mode
- `M112` : Halt
- `M114.1` : realtime position


Doing a toolchange:

```
M5 ; Stop spindle
T1 M6 ; Select tool 1 and do tool change
M3 S100 ; Start spindle at 100 RPM
```


