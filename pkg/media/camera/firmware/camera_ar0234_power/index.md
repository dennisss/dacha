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


## Uploading

- Use a high voltage programmer
- Configure the Arduino IDE with megaTinyCore
- Clock Frequency: 20Mhz
- No start up delay
- UPDI/RESET pin configured via fuse as a GPIO
    - This is used for the ENABLE_3V3 pin.


## Compiling (WIP)

```
~/Downloads/avr/avr8-gnu-toolchain-4.0.0.52-linux.any.x86_64/avr8-gnu-toolchain-linux_x86_64/bin/avr-gcc \
    pkg/media/camera/firmware/camera_ar0234_power/main.c \
    third_party/avr/megaTinyCore/megaavr/cores/megatinycore/main.cpp \
    third_party/avr/megaTinyCore/megaavr/cores/megatinycore/wiring.c \
    third_party/avr/megaTinyCore/megaavr/cores/megatinycore/wiring_digital.c \
    -I third_party/avr/megaTinyCore/megaavr/cores/megatinycore/ -mmcu=attiny402 \
    -I third_party/avr/megaTinyCore/megaavr/variants/txy2/ \
    -DF_CPU=20000000L \
    -DARDUINO_attinyxy2 \
    -Os -std=gnu11 -ffunction-sections -fdata-sections -MMD -flto -fno-fat-lto-objects \
    -DCLOCK_SOURCE=0 \
    -DMILLIS_USE_TIMERA0 \
    -DCORE_ATTACH_ALL \
    -DMEGATINYCORE="0" -DMEGATINYCORE_MAJOR=0 -DMEGATINYCORE_MINOR=0 -DMEGATINYCORE_PATCH=0 -DMEGATINYCORE_RELEASED=0
```


