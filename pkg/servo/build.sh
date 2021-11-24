#!/bin/bash
mkdir -p build
sdcc --Werror --std-sdcc99 -mstm8 --out-fmt-ihx -o build/main.ihx main.c