# Makera Carvera

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
    - UART uses a 16 byte TX and 16 byte RX FIFO buffer. 
- Co-processors
    - ESP M8266 module for WiFI
    - CC2530 module : For wireless probe communication.
- Micro SD card slot (labeld `TF Card`) wired directly to the main processor
- USB-C female poart (labeled `USB-UART`)
    - Normally routed to the machine back panel USB-C port
    - On the board, linked to an FTDI 232RL USB-to-Serial IC and then to the main processor
        - 128 byte buffer from the USB host to the UART
            - So ideally gcode lines stay below 128 bytes in length.
        - 256 byte 
            - So gcode responses should be less that 256 bytes in length.
    - FTDI chip supports up to a 3MHz baud rate. (divides a 48Mhz reference clock)  
- X,Y,Z axes
    - Main processor connected to external motor drivers via 2 x 74HCT245 logic transceiver ICs
- A,T axes
    - 2 x TMC2209 stepper motor drivers onboard.

## Firmware

The main processor runs Smoothieware with custom patches written by Makera and on top of those, we have additional patches to make it work well with serial monitoring software.

A prebuilt firmware binary can be find in [./firmware/carvera-firmware-r3.bin](./firmware/carvera-firmware-r3.bin).

### Compiling

Clone https://github.com/dennisss/CarveraFirmware

Compiling the firmware can be done with:

```bash
# Do once
./linux_install

# Do on every compile
./BuildShell
CNC=1 AXIS=5 make clean all
```

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
- Cable management
    - The cable from the camera can be routed along the left side of the dust bin and up the side of the side wall and then over the main control board towards the right side of the machine.
    - Then attach a 1ft USB A extension cable.
    - Then plug into a USB hub side as this [Anker 4-port hub](https://www.amazon.com/gp/product/B07L32B9C2).
    - Also plug in the control board serial to the hub using a new USB A to C cable.
    - Connect the USB hub to the rear USB-C port using a [USB C female to female adapter](https://www.amazon.com/gp/product/B0BCFPWQRP).
    - You can cut re-use one of the zip-tie holes along the rear top gap in the machine to hold the USB hub in place. 

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

- `G38.2 Z-10 F60`
    - Do leveling
    - Though I think its actually defaulting to 100mm/min?
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
    - This compiles to the following GCode:
        - `M497.5 ; Set ATC Status`
        - `G38.2 Z%.3f F%.3f`
            - `Z` can also be 'X' or 'Y'
            - Feed rate is 100mm/min (slow rate), 500mm/min (fast rate)
- `M370` : Clear auto bed leveling data set by `M32`
- `M375.1` : Display bed leveling data.

The leveling data gets rendered as follows:

```
29.5000| -0.0400 -0.1000 -0.1100 -0.1200 -0.0850
22.1250| -0.0400 -0.0850 -0.0900 -0.1000 -0.0550
14.7500| -0.0350 -0.0650 -0.0600 -0.0750 -0.0300
7.3750| -0.0150 -0.0400 -0.0250 -0.0300 0.0050
0.0000| 0.0000 -0.0300 0.0200 0.0100 0.0550
-----+----------+----------+----------+----------+-----
0.0000 28.6300 57.2600 85.8900 114.5200
```


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
- `src/libs/Kernel.cpp`
    - Contains the code that generates and state and diagnostic report strings send back to the host.
- DO NOT OVERRIDE `DEFAULT_SERIAL_BAUD_RATE` since this is also used for the internal serial connection to the CC2530.

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

### Endoscope Mount

This is a mount for attaching an endoscope camera to the Carvera to allow for visual alignment of double sided PCBs. Basically you need to drill at least 3 holes on the top side of your PCB all the way through, measure the centered position with the camera and then flip over the board and measure the wholes again. Then do some linear algebra (solve `A * top_points = bottom_points`) to figure out how to transform your back gcode. I do the alignment immediately after doing all the cutting on the top surface and keep a constant Z for all camera measurements to mitigate issues with the camera shifting around over time.

You should be able to achieve <0.1mm alignment precision with this method. Naturally the closer the camera is to the PCB, the bigger resolution you will get.

The 3d printed parts are made for different diameters of endoscopes. The one I mainly use is the [Teslong 5MP auto-focus camera](https://www.amazon.com/dp/B07HVT2XZL) which is 12.5mm in diameter. But, in hindsight the fixed focus ones would have been easier to use but harder to get in a higher resolution. You don't need the lighting from the endoscope as the light strips in the Carvera are sufficient to illuminate most boards.

Note that not all endoscopes support viewing from Windows/Linux/PCs. Keywords to search for are 'Windows', 'Linux', 'UVC' or presence of a male USB type A connector (the 3 in 1 connector endoscopes). Avoid iPhone endoscopes as they are probably not compatible with your computer.

Instructions:

- Print both parts in a strong filament
    - I used 100% infill PCCF with 0.95 extrusion multiplier and the 'print external perimeters first' setting to get dimensionally accurate parts
    - For the body, print with the endoscope hole on the print bed.
- Cut off the endoscope USB cable and solder on a USB-C breakout or just a USB-C cable.
- Attach the endoscope to the mount
    - WARNING: Some of the pictures are from an older version of the mount so the screws are backwards.
    - Use 2 x M2 16mm screws and corresponding nuts.  
    - Edge of endoscope should be at least ~22mm below the bottom of the 3d printed part. (Else the view will be obstructed with the laser or spindle)
- Before tightening the clamp, make sure that the camera is aligned straight with the mount by viewing it on your computer.
    - It is very annoying to adjust the orientation of the camera later.
- Attach the mount to your Carvera head by re-using the screw in the same position on the Carvera.
- Use a generous amount of hot glue for cable management.
- Re-cover the 

The v4l2 controls exposed by the recomended camera as the following:

```
The Endoscope

User Controls

                     brightness 0x00980900 (int)    : min=-64 max=64 step=1 default=0 value=0
                       contrast 0x00980901 (int)    : min=0 max=100 step=1 default=30 value=30
                     saturation 0x00980902 (int)    : min=0 max=128 step=1 default=54 value=54
                            hue 0x00980903 (int)    : min=-180 max=180 step=1 default=0 value=0
        white_balance_automatic 0x0098090c (bool)   : default=1 value=1
                          gamma 0x00980910 (int)    : min=100 max=500 step=1 default=300 value=300
                           gain 0x00980913 (int)    : min=0 max=128 step=1 default=70 value=70
           power_line_frequency 0x00980918 (menu)   : min=0 max=2 default=1 value=1 (50 Hz)
      white_balance_temperature 0x0098091a (int)    : min=2800 max=6500 step=10 default=4600 value=4600 flags=inactive
                      sharpness 0x0098091b (int)    : min=0 max=100 step=1 default=90 value=90
         backlight_compensation 0x0098091c (int)    : min=0 max=2 step=1 default=1 value=1

Camera Controls

                  auto_exposure 0x009a0901 (menu)   : min=0 max=3 default=3 value=3 (Aperture Priority Mode)
         exposure_time_absolute 0x009a0902 (int)    : min=1 max=10000 step=1 default=166 value=166 flags=inactive
     exposure_dynamic_framerate 0x009a0903 (bool)   : default=0 value=1
                   pan_absolute 0x009a0908 (int)    : min=-57600 max=57600 step=3600 default=0 value=0
                  tilt_absolute 0x009a0909 (int)    : min=-43200 max=43200 step=3600 default=0 value=0
                 focus_absolute 0x009a090a (int)    : min=0 max=990 step=1 default=68 value=68 flags=inactive
     focus_automatic_continuous 0x009a090c (bool)   : default=1 value=1
                  zoom_absolute 0x009a090d (int)    : min=0 max=3 step=1 default=0 value=0


```

It is recommended to disable `focus_automatic_continuous` once the camera is initially focused.


### Solder Mask

- Mechanic `UVH900-BY` is what the Carvera PCB frab kit comes with.
- `SUNmini Plus` UVLED Lamp
    - 24W 5V
    - 365 + 405nm light
    - 
- Solder mMask remover
    - 0.3mm x 30degree
    - RPM:6000
    - Feed:400
    - PFeed:200
    - DOC:0.2 (same as in CAM guide)
    - Should treat as a 0.3mm diameter cutter though


PCB recommendations based on https://wiki.makera.com/en/software/MakeraCAM_userguide

- Line width > 0.2mm
Line spacing > 0.2mm
Via diameter >= 0.4mm
Silk screen line width > 0.25mm
Drill unit to metric mm 3.3 format
And the smallest flat bottom tip tools that can be used are 0.1mm\60° or 0.2mm\30°


- 0.05mm depth for the engraving
