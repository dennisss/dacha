# Carvera

The [Carvera](https://www.makera.com/products/carvera) is a desktop CNC milling machine machine sold by Makera. 

## Recommended Setup

- Add camera mount
- USB Hub internally with USB-C to A adapter
- Data-only USB

## Board

The main control board can be found behind the back panel of the machine. The board is enclosed in the below box:

![](./images/board-cover.jpg)

Under the cover is the main PCB that looks like below:

![](./images/board.jpg)

Technical details of the board:

- Main Processor: LPC1768FBD100
- Co-processors
    - ESP M8266 module for WiFI
    - CC2530 module : For wireless probe communication.
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

DO NOT OVERRIDE `DEFAULT_SERIAL_BAUD_RATE` since this is also used for the internal serial connection to the CC2530.

Compiling the firmware can be done with:

```bash
# Do once
./linux_install

# Do on every compile
./BuildShell
DEFINES="-DNO_TOOLS_EXTRUDER " make clean all
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

- Even more verbose communication can be found in LinuxCNC here: https://linuxcnc.org/docs/html/gcode/g-code.html

TODO: Mirror the above pages.

### Coordinate System

On startup, the machine homes itself at the X,Y,Z max limits (0, 0, 0) which becomes the origin of the 'machine coordinate system':

- (0, 0, 0) is the max position at the upper right of the machine limited by hardware end stops,
- (-370, -250, -135) is the min position and limited by software end stops.
- Standard clearance position is at (-75, -3, -3)
- Anchor 1 (inner corner of L-bracket) is at (-360.158, -234.568)

Other coordinate systems can be controlled as followed:

- `G53 ...` uses machine coordinates for one line of code.
- `G54`, `G55`, `G56`, ... switch to coordinate systems #1, #2, #3, ... 
- `G10 L2` can be used to configure the offset of a coordinate system relative to the machine origin.

### Operations

#### Buffering

Lines can be rpefixed by `buffer ` to buffer the command for later execution. See the smoothieware `Player.cpp`.

#### Movement

- `M496.1` : Move to clearance position
    - Supposedly `G28` also works.
- `M496.2` : Move to work origin
- `M496.3` : Move to 'Anchor 1'
    - This position is on the inner corner of the L-bracket (+15mm, +15mm) away from the bottom left edge of the bed.
    - State: `b"<Idle|MPos:-359.7550,-234.2900,-123.0000,0.0000,0.0000|WPos:-52.0000,-37.5000,-48.1850|F:0.0,3000.0,100.0,29.1|T:6,-13.510|W:3.94|L:0, 0, 0, 0.0,100.0|M:29.1,0.0>\nok\r\n"`
- `M496.4` : Move to 'Anchor 2'
- `M496.5 X??Y??` : Move to 'Path Origin'.
    - TODO: What are the parameters.

#### Leveling

Example command for doing full mesh leveling:

- `M496.3` : Move to anchor 1
- `G54` : Use coordinate system #1
- `G10 L2 P1 X-360.158 Y-234.568 Z-3` : Set anchor 0 to be (0,0,0)
- `M495 X15 Y15 C100 D60 O0 F0 A85 B45 I3 J3 H5 P1`

full reference:

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
- `G10 L10` can be used to set the tool offsets.

TODO: Figure out how to 

#### Wireless Probe

To pair the wireless probe, start by running `M471`.

Press the wireless probe manually for 10 seconds (or until the green LED starts to blink).

If wireless pairing succeeds, the green LED will blink 5 times slowly. In either case, the green LED will switch off at the end of pairing (times out after 30 seconds).

If successful, the machine will print `WP PAIR SUCCESS!` over serial. Else it will print `WP PAIR TIMEOUT`:q

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
- `M494.2` : Probe laser off?
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
G21 G40 G54           % mm mode, turn off cutter compendation?, use workspace coordinate system #1
G80 G90 G94           % cancel canned motions, absolute mode, units per minute feed rates
( Tool #2 "30degree0.2mm" / Diameter 3.175 mm )
T2 M06                % Tool change. Note that "T" is considered a parameter to the "M" command
M03 S12000            % Start the spindle.
M07                   % Start airflow
G00 X114.096 Y0.714
```

Typical end gcode:

```
G00 Z3
M09                  % Turn off airflow
M05                  % Stop spindle
M02                  % End program
%
```

## Software

Smoothieware Firmware:

- `src/modules/communication/SerialConsole.cpp` : Code for the main serial console receiving USB data.
    - Emits `ON_CONSOLE_LINE_RECEIVED` events from the USB.
- `src/modules/communication/GcodeDispatch.cpp`
    - Subscribes to `ON_CONSOLE_LINE_RECEIVED`
    - This subscriber is the one that returns `ok` messages. 
- `src/modules/tools/atc/ATCHandler.cpp` : Tool changer and leveling scripts
    - Subscribes to `ON_CONSOLE_LINE_RECEIVED` and emits additional events to run scripts.
- `src/modules/communication/SerialConsole2.cpp` : Wireless probe communication logic.
    - Subscribes to `ON_CONSOLE_LINE_RECEIVED`

Carvera Controller

- Uses https://github.com/kivy/python-for-android
- All the source code in the APK is in `private.mp3` which is a tar file.

## Serial Protocol

Smoothieware uses a 256 character serial buffer. Meanwhile gRBL only guarantees a 128 character serial buffer.

Startup logs: (these are the lines printed when the machine first boots up)

- `b"version = 0.9.7\n"`
- `b"Watchdog enabled for 10.000 seconds\n"`
- `b"ok\nG28 means goto clearance position on CARVERA\n"`
    - The `ok` comes from `home_on_boot` being true by default which causes a `$H` command to run.
- `b"STA connection timeout, disconnected!\n"`

Querying the current state (and response when first powered up):

- Request: `?`
- Response: `<Idle|MPos:-75.0000,-3.0000,-3.0000,0.0000,0.0000|WPos:232.7550,193.7900,71.7950|F:0.0,3000.0,100.0,25.8|T:6,-13.490|W:0.00|L:0, 0, 0, 0.0,100.0|M:25.8,0.0>\nok - ignore: []\n`
    - Note: gRBL does not guarantee that this will get a response.
    - gRBL recommends running this at no more than 5Hz for real time feedback.
- Response with wireless probe connected: `<Idle|MPos:-75.0000,-3.0000,-3.0000,0.0000,0.0000|WPos:232.7550,193.7900,71.8150|F:0.0,3000.0,100.0,29.0|T:6,-13.510|W:4.02|L:0, 0, 0, 0.0,100.0|M:29.0,0.0>\n`

Querying spindle temperature:

- Request: `M105\n`
- Response: `ok M:26.1 /0.0 @0 \r\n`

View gcode parser state:

- `$G`
- `[G0 G54 G17 G21 G90 G94 M0 M5 M9 T0 F3000.0000 S1.0000]\nok\n`

View parameters:

- Request: `$#`
- Response: `[G54:-307.7550,-196.7900,-61.3050]\n[G55:0.0000,0.0000,0.0000]\n[G56:0.0000,0.0000,0.0000]\n[G57:0.0000,0.0000,0.0000]\n[G58:0.0000,0.0000,0.0000]\n[G59:0.0000,0.0000,0.0000]\n[G59.1:0.0000,0.0000,0.0000]\n[G59.2:0.0000,0.0000,0.0000]\n[G59.3:0.0000,0.0000,0.0000]\n[G28:0.0000,0.0000,0.0000]\n[G30:0.0000,0.0000,0.0000]\n[G92:0.0000,0.0000,0.0000]\n[TL0:-13.4900]\n[PRB:0.0000,0.0000,0.0000:0]\nok\n`

View diagnostic report:

- Request: `*`
- Response: `{|L:0,0|V:0,0|F:0,0|G:1|T:0|R:0|C:1|E:0,0,0,0,0,1|P:0,0|A:0,0|I:0}\nok\r\n`

Performing a tool change:

- Request: `T1M6\n`
    - Both commands MUST be on the same line
- Response:

    ```
    b"Start atc, old tool: T6, new tool: T1\r\nok\r\nM497.1\r\nok\r\nG53 G0 Z-3.000\r\nok\r\nG53 G0 X-3.755 Y-234.290\r\n"
    b"ok\r\nM492.2\r\n"
    b"ok\r\n"
    b"G53 G0 X-3.755 Y-234.290\r\nok\r\nG53 G1 Z-97.230 F1000.000\r\nok\r\nG53 G1 Z-112.230 F200.000\r\nok\r\nM490.2\r\nHoming atc...\n"
    b"ATC homed!\r\n"
    b"ATC loosed!\r\nok\r\nG53 G0 Z-50.000\r\nok\r\nM493.2 T-1\r\n"
    b"ok\r\nM492.1\r\n"
    b"ok\r\nM497.2\r\n"
    b"ok\r\nG53 G0 Z-50.000\r\nok\r\nG53 G0 X-3.755 Y-84.290\r\n"
    b"ok\r\nM492.1\r\n"
    b"ok\r\nM490.2\r\nAlready loosed!\nok\r\nG53 G0 X-3.755 Y-84.290\r\nok\r\n"
    b"G53 G1 Z-97.230 F1000.000\r\nok\r\nG53 G1 Z-112.230 F200.000\r\nok\r\nM490.1\r\n"
    b"ATC clamped!\r\nok\r\nG53 G0 Z-20.000\r\nok\r\nM492.2\r\n"
    b"ok\r\nM493.2 T1\r\n"
    b"ok\r\nM497.3\r\nok\r\nG53 G0 Z-20.000\r\nok\r\nG53 G0 X-3.755 Y-54.290\r\nok\r\nG38.6 Z-152.230 F500.000\r\n"
    b"[PRB:-3.755,-54.290,-82.840:1]\nok\r\nG91 G0 Z2.000\r\nok\r\nG38.6 Z-3.000 F100.000\r\n"
    b"[PRB:-3.755,-54.290,-82.820:1]\nok\r\nM493.1\r\n"
    b"ok\r\nG53 G0 Z-20.000\r\n"
    b"ok\r\n"
    b"Done ATC\r\n"
    ```


Inherited from gRBL

- `!`: Feed hold. Gracefully decelerates to a stop. Does not alter the spindle state.
- `\x18`: Soft-reset. If currently in motion, we will enter an alarm mode and lose the position
- `M112\n`: Halt everything
- `$X` | `M999\n` : Reset alarm state. 

### Challenges

Carvera/Smoothieware specific challenges for supporting monitoring software:

- We can't differentiate between the 'ok' responses for internal macro sub-commands and commands sent by a computer over USB
    - Solution will be to prefix responses with `>` (partially mimics the gRBL startup line format).
- Gcode lines can have multiple commands in one line (e.g. `M6T6`)

### TODOs

- Need to detect response lines with 'alarm' in them.
- Need to figure out when we lose the known position

