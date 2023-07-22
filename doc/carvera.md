
## Overview

The Carvera uses a custom board via an LPC1768 running Smoothieware. Also on board is a:

- M8266 module for WiFI
- CC2530 module (currently unused?)

## Disabling WiFi

First connect to the WiFi and then run the following commands:

```bash
nc 192.168.4.1 2222
config-set sd wifi.enable false
```

On restart, the WiFi network will still be advertised but connecting to it won't allow controlling the device.

## GCode Reference

- `M999` : Reset from halted/alarm state.
- `$H` : Home all axes.
- `G21` : Set to millimeter mode
- `M112` : Halt
- `M114.1` : realtime position


Doing a toolchange:

```
M5 ; Stop spindle
T1 M6 ; Select tool 1 and do tool change
M3 S100 ; Start spindle at 100 RPM
```

## Notes

UART: Can go up to 3MB (limited by FT232 chip)


```
$ config-load checksum wifi.enable
checksum of wifi.enable = 50B0 7369 00

$ config-load checksum wifi.machine_name
checksum of wifi.machine_name = 50B0 89D9 00

$ config-load checksum acceleration
checksum of acceleration = 62EE 00 00
```


