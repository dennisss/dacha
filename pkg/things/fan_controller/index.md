

HL15 Fan Controller:

- nRF52 ItsyBitsy
- Top row: Fan 1, Fan 3, Fan 5, Fan 6, Fan 7
- Bottom row: Fan 2, Fan 4, Fan 8

- Must pull up tachometer inputs

- D13 / P0.12 : Fan 1/2 PWM
- D12 / P0.11 : Fan 1 Tachometer
- D11 / P0.26 : Tan 3/4 PWM
- D9 / P0.07 : Fan 3 Tachometer
- D7 / P0.08 : Fan 5/6 PWM
- SCL / P0.14 : Fan 5 Tachometer
- SDA / P0.16 : Fan 6 Tachometer
- D1 / P0.24 : Fan 7/8 PWM
- D0 / P0.25 : Fan 7 Tachometer
- A0 / P0.04 : Fan 2 Tachometer
- A2 / P0.28 : Fan 4 Tachometer
- MISO / P0.20 : Fan 8 Tachometer

Board specs:
- 32kHz crystal on XL1/XL2
- Separate IC for doing 3.3V regulation.
- Red LED : (High voltag = on) : P0.06
- Dot Data: P0.08
- Dot CLK : P1.09
    - APA102-202
    - This basically uses SPI
        - https://learn.sparkfun.com/tutorials/apa102-addressable-led-hookup-guide/all
- User Switch : P0.29
    - Must pull up. Connects to GND on user press
    - APA102-202

ItsyBitsy Specs
- https://learn.adafruit.com/adafruit-itsybitsy-nrf52840-express/downloads
- Schematic: https://cdn-learn.adafruit.com/assets/assets/000/087/158/original/adafruit_products_schem.png?1579387035

nRF52840 Feather
- https://learn.adafruit.com/introducing-the-adafruit-nrf52840-feather/downloads
- https://cdn-learn.adafruit.com/assets/assets/000/068/545/original/circuitpython_nRF52840_Schematic_REV-D.png?1546364754\


Requirements for launch:

- Must allow setting back to default value if enough time elapses with no update to the value.
    - Periodically update things.
- Want to have some fan curve support.