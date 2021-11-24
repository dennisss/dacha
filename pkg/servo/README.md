# Servo Motor Firmware

This is an alternative firmware designed for use in CLS6336HV servo motors which uses a re-programmable STM8S003F3 8-bit microcontroller.

## Features:

- I2C 400Kbit/s control and feedback interface (bit-banged)
- Velocity, torque, and position control modes.
- PID controller with tunable weights.

TODO:
- Stall detection


## Old


MT4953A

- Read current position from ADC potentiometer
- Read desired position from the PWM signal
- Diff and get a control signal as a PWM signal to the DC motor 
    - Depending on the sign, enable/disable the appropriate gates (PWM controlling 1 half of it)

- Feedback to the controller
    - Stall detection: Can be done with a expected motion threshold or a current sense.
        - Need not be instantanious
        - Issue: Stale means they 


Available pins:
- SWIM - PD1
- (currently mapped to PWM) PC5 - TIM2_CH1


./stm8flash -c stlinkv2 -p stm8s003f3 -r flash.bin

stm8s003f3


PWM to motor: 20 kHz 