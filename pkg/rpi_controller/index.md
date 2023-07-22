# Raspberry Pi Fan Controller

This is a simple program to control a single PWM fan connected to a Raspberry Pi.

- Uses the native PWM peripheral and not software PWM
- Automatically regulates fan speed based on a CPU temperature to speed curve.
- Supports inspection and overriding of the fan value via 


V2 stuff:

- I2C1 (pin 3/5)
- RTC power: pin 7 : Drive low to turn up.
- Fan PWM: pin 12 (GPIO 18)
    - Use for the regular PWM
- Fan Tach: pin 11
    - Use GPIO interrupts to detect the period
    - Peak 5000 RPM
    - Up to 166Hz signal 
    - So ~6ms between high/low edge.
    - Is puled up by us.
    - So measure time between two consecutive low pulses.
- LED Serial: pin 40 (PCM DOUT)
    - 
- AUX PWM: pin 33


https://github.com/jgarff/rpi_ws281x
