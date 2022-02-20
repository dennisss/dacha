

--- a/firmware/grbl_port/common_src/config.h
+++ b/firmware/grbl_port/common_src/config.h
@@ -211,7 +211,7 @@
 // will be applied to all of them. This is useful when a user has a mixed set of limit pins with both
 // normally-open(NO) and normally-closed(NC) switches installed on their machine.
 // NOTE: PLEASE DO NOT USE THIS, unless you have a situation that needs it.
-// #define INVERT_LIMIT_PIN_MASK ((1<<X_LIMIT_BIT)|(1<<Y_LIMIT_BIT)) // Default disabled. Uncomment to enable.
+#define INVERT_LIMIT_PIN_MASK ((1<<X_LIMIT_BIT)|(1<<Y_LIMIT_BIT)) // Default disabled. Uncomment to enable.
 
 // Inverts the spindle enable pin from low-disabled/high-enabled to low-enabled/high-disabled. Useful
 // for some pre-built electronic boards.
diff --git a/firmware/scripts/configurable_compile_script.py b/firmware/scripts/configurable_compile_script.py
old mode 100644
new mode 100755


ER8M chuck with 1/8" chuck
- Compatible with Nomad 3 bits.


Unknowns:
- How to set the current output of the stepper drivers?



Pulleys:
- 16 Tooth 6mm x 2 are genuine

180 - 2 * 2

=> 176mm x 2 for the main rods

2 x 90mm 


TODO: Check the size of the gaskets and the disk nut that connects to collet.


First board to make is the desk controller:
- NRF52840 SMT
- 4 pin debug header
- 2 level shifters for the TX/RX lines. Each with:
    - 1 2N7000 transistor
    - 2 10K resistors.
- 4 pin header for attaching the RJ-11 connection
- Also an EEPROM

## Reflow Oven

Past work:
- https://learn.adafruit.com/ez-make-oven/putting-it-all-together
- https://www.whizoo.com/reflowoven


## Using SLA printing LCD

Past work:
- https://hackaday.io/project/178451-qwicktrace-pcb




Sainsmart CNC 3020 settings

- Isolation Routing Copper Traces
    - 0.15mm Tool Diameter (or 0.21 if using 60 degree)
    - V-Bit 30 degree with 0.1mm diameter
    - 3 passes with 10% overlap
    - Travel Z: 2mm
    - Feedrate X/Y: 120 mm/min
    - Feedrate Z: 60 mm/min
    - Spindle: 10000 RPM
    - Rapid Move Feedrate: 1500 mm/min
- Drilling
    - Travel XY feed rate: 1500
    - Z feed rate: 40
    - Spindle Speed: 10000 RPM
    - Z Cut Position: -1.7
    - Z Move Position: 2
- Routing Edge:
    - 1.2mm bit (3rd from smallest)
    - 1.7mm cut depth, 0.5mm per pass (4 passes)
    - 0.1mm margin
    - No gaps
    - 60mm/min XY cutting speed
    - 40mm/min Z cutting speed
    - Travel Z: 2mm


## Old


Settings using 3018
- How to do mesh leveling/
- Engraving settings from 'Teaching Tech'
    - For 20 degree v cutter
    - Tool number 10
    - 0.15mm depth
    - 254mm/min feed rate
    - 50mm/min plunge rate
    - 1000 RPM
    - 20% step over

- Documentation from Sainsmart
    - https://docs.sainsmart.com/3018-prover
    - https://docs.sainsmart.com/3018-prover-offline


- Should install grblcontrol
    - from https://github.com/Denvi/Candle
    - `mkdir build`
    - `cd build`
    - `qmake ../src/candle.pro`
    - `make -j4`

- Installing flatcam
    - `git clone https://bitbucket.org/jpcgt/flatcam`
    - `git checkout origin/Beta`
    - `./setup_ubuntu.sh`
    - `pip3 install -r requirements.txt`
    - `python3 FlatCAM.py`
    - Must have vispy at 0.6.6
        - Can fix this by changing the requirements.txt and re-installing.
        - See https://gist.github.com/natevw/3e6fc929aff358b38c0a

- Creating ubuntu live usb
    - `sudo apt install usb-creator-gtk`


- By default, speed rates would be from 0-1000
    - https://docs.sainsmart.com/article/9m0rbnw6k1-introduction-to-cnc-for-a-total-novice-tuning-gbrl-settings
    - Could set tothe actual RPM range.
