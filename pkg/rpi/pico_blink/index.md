# Pico 2 Blink Project

This project provides a basic blink application for the Raspberry Pi Pico 2, as well as a conditionally-compiled low power variation.

## Compilation Instructions

The project uses CMake and requires the Pico SDK. A `build.sh` script is provided to compile both `pico_blink` and `pico_blink_low_power`.

To build the project:
1. Make sure you are in the project root directory.
2. Run the build script (which internally sets `PICO_SDK_PATH` and target board):
   ```bash
   ./build.sh
   ```
3. The resulting firmware binaries (`pico_blink.uf2` and `pico_blink_low_power.uf2`) will be located in the `build/` directory. Flash them onto the Raspberry Pi Pico 2 board by holding the `BOOTSEL` button when plugging it into USB, and copying the `.uf2` file.
