Components to get:
- NRF52 system
- STM32 system
- RP2040 system

- LM311
- TS391
- LM358


The Ant PCB Maker

25 x 11

- X/Y
    - Motors are 'NEMA11 2-Phase 1.8 Degree Stepper Motor 1200g.cm/0.67A'
        - Wires:
            - Black (coil 1 +)
            - Green (coil 1 -)
            - Red (coil 2 +)
            - Blue (coil 2 -) 
    - Using GT2 16T pulleys so 6.25 full steps per mm.
- Z
    - Motor is a NEMA 8 2-phase 1.8 degree stepper motor
    - Max 0.6A current
    - Wires:
        - Green (coil 1)
        - Yellow (coil 1)
        - Black (coil 2)
        - Red (coil 2)
    - Tr4.76 0.635mm pitch
        - 314.96 full steps per mm



- General limits to aim for:
    - 100mm/s
    - 1000mm/s^2

- 625 full steps performed per second
    - 10K individual steps per second (at 1/16 microstepping)
    - 


GT2 Pulley 16T
- GT2 belt is 2mm bitch.



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



- Creating ubuntu live usb
    - `sudo apt install usb-creator-gtk`

