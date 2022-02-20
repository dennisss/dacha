
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




TODO: Use a relay to control the Pi as this is probably lower voltage drop? but will have 


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

Power Supply
- UHP-200R-5

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
