# Raspberry Pi Peripheral Controller

This program is meant to run as a daemon on a headless Raspberry Pi to monitor overall system health and control shared peripherals like cooling fans and identifier LEDs.

Current list of highlighted features:

- 4-pin PWM fan speed setting and tachometer input monitoring support
- System temperature readouts
- GPIO or WS2812 LED support



## Old

Things I want to expose:

Entities:

- CPUs
- Fans
- LEDs
- Temperature Sensors (mainly the CPU one)
- CPU Temperatures

Output numbers:

- Fan active duty cycle
- Fan measured RPM
- CPU { frequency, min_frequency, max_frequency }
- CPU temperature

Controllable state:

- Fan auto bool
- Fan manual duty cycle
- LED script







- Metrics:
    - Fan RPM
    - cpuN_frequency
    - CpuN_min_frequency
    - cpuN_max_frequency
- 

- Fan curve/speed
- RPM readout
- CPU frequencies
- RGB colors for LEDs
- CPU temperatures
- Support for pulsing the GPIOs

TODO: Need nodes to have 'labels' to identify if pi-rack is prevent

Unrelated things I need:
- 


Read CPU frequency via 

cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq 


/*
Cur CPU frequency:
- /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq
*/
