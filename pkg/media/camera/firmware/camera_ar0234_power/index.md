# AR0234 Power Sequencing Firmware

This is firmware for the ATTiny402 MCU on the AR0234 board to sequence power on/off of the sensor's power rails and crystal.

The MCU is powered from 3.3V.

Pinout:
- PA0: ENABLE_3V3 (digital input)
- PA1: RESET_1V8 (digital output) : externally pulled up to 1.8V. Must not exceed 1.8V (must only be high-z or low on the MCU)
- PA2: ENABLE_1V8 (digital output. 3.3V capable) : externally pulled down to GND
- PA3: EXTCLK_ENABLE (digital output) : externally pulled up to 1.8V (MCU should initially drive this low before doing anything else to keep the clock disabled)
- PA6: ENABLE_2V8 (digital output) : externally pulled down to GND.
- PA7: ENABLE_1V2 (digital output) : externally pulled down to GND.

TODOs:
- Add a resistor pulldown for the ENABLE_3V3 pin

