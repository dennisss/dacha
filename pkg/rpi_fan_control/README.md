# Raspberry Pi Fan Controller

This is a simple program to control a single PWM fan connected to a Raspberry Pi.

- Uses the native PWM peripheral and not software PWM
- Automatically regulates fan speed based on a CPU temperature to speed curve.
- Supports inspection and overriding of the fan value via 