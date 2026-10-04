# VB5943 Camera Board


Power Rails:

- VANA (2.8V analog power)
    - Typical: 38.5mA
    - Max: 40mA
- VCORE (1.15V digital core voltage)
    - Note: 1.2V should work for this.
    - Typical: 178mA
    - Max: 290mA
- VDDIO (1.8V I/O voltage)
    - Typical: 0.52mA
    - Max: 0.6mA



Power Sequencing
- Switch on power
- Switch RESETN from low to high
- ~6ms after reset, the device is good to use.