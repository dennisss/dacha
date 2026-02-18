


Top hat connections:
- I2C for screen
- I2C for Accelerometer
- I2C for thermal
  - - 2 pins
- SPI for DWM
  - Also an extra GPIO for deep sleep + IRQ
  - 4 + 2
- PWM for IR LEDs
  - 1
- Serial for Neopixel Ring
  - 1
- Extra IO for shutter swap
  - 1

- So total is 12 (also add another 2 for extra 5V and GND)
  - Can reduce to UART + 5V + 3V + GND (5 pins) if we use an external IC

TODO: In Mocap code, lock the CPU frequency.

Pi Pins are ~6mm above board
=> So need another 34mm of rise
=> Minimum is 28mm

Arducam camera boards are max 40x40 mm
=> Holes at 34x34 with each being M2