
Assembly:
- Assemble the 2U case
  - attach to Ikea Lack with 4 x (M5 60mm) screws
- Single tray
  - Laser cut 1 tray (DXF is in inch units) out of ~3mm thick acrylic.
  - Tap all small holes with an M3 tap.
  - 4 pi-mounts
    - Glue these with Gorilla glue to the tray
    - Use 4 x M2.5 8mm screws to fix the Pi to the tray
    - 
  - Print 1 fan-mount
    - Supports Noctua 40mm x 10 or 20mm fans (use the appropriate holes for either)
    - Screw on the fan with regular noctua self-tapping screws
    - Use 2 x M3 6mm screws to fix it to the tray
- 'Standard Pi 4 Armor'
  - Comes with 4 x M2.5 8mm screws 

New board design:

- POE diodes and module connector
  - CD-HD201
  - AG5405
- TPM over I2C
  - Infineon OPTIGA TRUST M SLS 32AIA
- RTC over I2C
  - DS3231: +/- 5ppm
  - CR1220 battery for this.
  - Battery voltage monitor for this.
- Fan connector (capability to monitor speed as well as control speed)
  - Noctua NF-A4x20mm fans have 26 AWG wiring 
  - Noctua NF-A4x10mm fans have 28 AWG wiring
- 1 LED (maybe RGB side-illuminated)
- OLED 128x32
  - I2C
- General purpose button
- Export a few of the I2C or SPI pins from the Raspberry Pi.

Fuse:
- 2JQ 3-R
    - https://www.digikey.com/en/products/detail/bel-fuse-inc/2JQ-3-R/1009870
    13.8mm by 5.1mm
    067R
- Fuse Clips
    - FC-203-22
    - https://www.digikey.com/en/products/detail/bel-fuse-inc/FC-203-22/2650843?s=N4IgTCBcDaIGIGEC0YAMBmFEC6BfIA
- Capacitors:
    - https://www.digikey.com/en/products/detail/nichicon/UVK1E101MDD1TD/4328641

Short circuit protection:
- Ceramic fuse
    -

100 + 3.5 + 29 - 9.5*2.54

100 + 3.5 + 2.54/2

80mm total height

POE module needs 19.5mm
Pi is 56

2.35 side pad

110.25 + ((103.685 - 97.335) / 2)


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

Main TODOs
- Verify all old raspberry pis have the UART in the same spot

TODO: Use a relay to control the Pi as this is probably lower voltage drop? but will have 


Center of 16 pin at 139
14mm wide lens

16 AWG stranded power distribution by 450mm
- Around 0.01 ohms
    - Expectation is up to 0.2V drop over the wire at 5V 20A.
    - So up to 4 watts of power disipation (96% efficiency.)
- At 12V, this goes down to 0.7 Watts
    - 99% efficiency

Decisions:
- Use 12V or 5V for system power
    - 12V would require additional DC/DC conversion.
        - For a single rack, probably not worth the cost.
- PTC fuse vs glass fuse
    - PTC fuse
        - Time to trip is O(100ms) - O(seconds)
        - Maybe around 0.05 ohms
- TVS diode
- 1000uF capacitor on each pi

(122, 128)

Power Supply
- UHP-200R-5

103.5 + 29 - (2.54*9.5)

126.6 + 1.55

LED Controller:
- MPQ3362

PCB should support either bridging input power to 5V or 


- ACS712 / ACS723
    - Supply current is 10mA, 1mOhm burden
    - ~0.059 watts for 1 Pi at 5V/3A
    - 200-400mV/A
- INA139/169
    - With 0.01 ohm resistor (0805)
        - 0.09 watts for 1 Pi at 5V/3A
        - 0.03 volt raw output
        - *100 to get 
        - A 100K load resistor would give the desired 100 gain 
        - An NRF52840 has a >1MOhm input resistance
        - NRF52840 internal reference is 0.6V
            - Really only need a gain of around 20 (use a 20K resistor)
        - RP2040 input impedance is >100kOhm

- Measuring input voltage
    - GPIO input voltage can not exceed VDD + 0.3v
    - Cut the input voltage in half

- Useful posts:
     -https://devzone.nordicsemi.com/nordic/nordic-blog/b/blog/posts/measuring-lithium-battery-voltage-with-nrf52
    - https://devzone.nordicsemi.com/nordic/nordic-blog/b/blog/posts/measuring-lithium-battery-voltage-with-nrf51

- Overvoltage protection?
    - Simple solution is to have the NRF monitor input current and voltage at 100Hz and power off the pi if detecting a huge spike

- ACS712ELCTR-20A-TCopy

- PoE is better for power isolation


Challenges:
- Before the fuse blocks, other nodes can pull power from the capacitor
- Use diode to power the NRF


- TPS2296xC

TODO: Need to make sure that I also measure the input voltage.


Suppose 
- Issue is that low side mosfets will reduce the ground voltage
- Will end up being at 0.1 volts


Lite variation:
- 1 Pi I2C dedicated to the HDMI
- 1 Pi I2C used for RTC
- 1 Pi I2C used for the Rack
