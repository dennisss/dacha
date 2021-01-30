#!/bin/bash
avrdude -v -patmega32u4 -P/dev/ttyACM1 -b57600 -cavr109 -D -Uflash:w:target/atmega32u4/release/fan_controller.elf:e