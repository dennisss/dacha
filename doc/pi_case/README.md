

Pi PWM connection
- Tachometer output to GPIO 6
- PWM to GPIO 12 (PWM0)

For cluster
- Control interface
    - 4 pin JST XH
        - 12V
        - SCL
        - SDA
        - GND
- 40mm Noctua PWM fan
    - Connected to Raspberry Pi for temperature control
    - Is simpler to add to the interface IC to support 
- Internal RTC
- Use an AT-Tiny
    - (2 pins) Communicates over I2C
    - (1 pin) Current sensing analog pin
    - (1 pin) Power control
        - Support turning on the raspberry pi
        - The currently toggled power on/off setting will be saved in internal eeprom
        - In order to disable a Pi, the AT-Tiny must get a second 'confirm' request within 2 seconds
            - The I2C controller should send it after 1 second (this is a safe guard to prevent a Pi from turning off itself)
        - For simplicity, the I2C 
    - (1 pin) Voltage sensing analog pin
- 3A Resetable fuse on 5V
- 12V to 5V step down.

- Consider opto-isolation of the I2C pins

- This means we may want to also run the AT-Tiny85 at 3.3V to avoid having to switch it to use 

- ACS723
    - 400 mM/A
    - So ~1200 mV over entire range we care about.
    - Applification would be ice.

AT-Tiny Pins
- RESET pin: 


Measure up to 1.5A on input 24V line


AT-Tiny Programming protocol
- Tiny Programming Interface (TPI)
- http://ww1.microchip.com/downloads/en/AppNotes/doc8373.pdf
- https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-2586-AVR-8-bit-Microcontroller-ATtiny25-ATtiny45-ATtiny85_Datasheet-Summary.pdf
- https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-2586-AVR-8-bit-Microcontroller-ATtiny25-ATtiny45-ATtiny85_Datasheet.pdf



In between the legs of the ikea lack is 445mm

Goal is 500mm for beams
- So 27.5 into each block



I want the thing to be 150mm lng.


200 + 300

445


150mm total length

Screw goes into 1515 extrusion by up to 3mm


nrf52840
- aQFN72
- DCDCEN0 should be set.
- Power from VDDH
- Don't really care about USB.
- Use Config reference no 2.

- The main things I need are:
    - I2C: Uses HLCLK
    - Timers
    - RTC: Uses LFCLK

Possibly try MP2307 (up to 95% efficiency?)
