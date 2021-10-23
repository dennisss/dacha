
Settings up headless Raspberry Pi

Based on https://www.raspberrypi.org/documentation/configuration/wireless/headless.md

Place a `wpa_supplicant.conf` in the root of the boot partition. Should contain contents like:

```
ctrl_interface=DIR=/var/run/wpa_supplicant GROUP=netdev
country=US
update_config=1

network={
 ssid="<Name of your wireless LAN>"
 psk="<Password for your wireless LAN>"
}
```

Based on https://www.raspberrypi.org/documentation/remote-access/ssh/README.md
- Place an empty file named `ssh` into the root of the boot partition.

TODO: Later try https://pibakery.org/download.html

Run sudo `raspi-config` and go to `6 Advanced Options` to run `A1 Expand Filesystem`

`sudo apt install python3-gpiozero`

My fan connection:
- PWM0 output on GPIO 12
   - GPIO/BCM pin 12
   - Alt0
- Tachometer input in GPIO 6


The `cross` tool exists to cross compile:
- https://github.com/rust-embedded/cross
- `cargo install cross`
- `cross build --target=armv7-unknown-linux-gnueabihf`
- Useful guide: https://capnfabs.net/posts/cross-compiling-rust-apps-raspberry-pi/


MMAL header
- https://github.com/raspberrypi/userland/blob/master/interface/mmal/mmal.hs
- Headers in /opt/vc/include/interface/mmal


Bindgen dependenceies:
- `apt install llvm-dev libclang-dev clang`


Fan
```
import gpiozero
>>> gpiozero
<module 'gpiozero' from '/usr/lib/python3/dist-packages/gpiozero/__init__.py'>
>>> led = gpiozero.PWMLED(pin=12, frequency=25000)
>>> led
<gpiozero.PWMLED object on pin GPIO12, active_high=True, is_active=False>
>>> led.value = 0.9

```



HDMI

```
pi@raspberrypi:~ $ sudo i2cdetect -y 1
     0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f
00:          -- -- -- -- -- -- -- -- -- -- -- -- -- 
10: -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- 
20: -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- 
30: -- -- -- -- -- -- -- 37 -- -- 3a -- -- -- -- -- 
40: -- -- -- -- -- -- -- -- -- -- 4a 4b -- -- -- -- 
50: 50 -- 52 53 54 -- -- -- -- -- -- -- -- -- -- -- 
60: -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- -- 
70: -- -- -- -- -- -- -- --   
```

```
pi@raspberrypi:~ $ ddcutil detect
Display 1
   I2C bus:             /dev/i2c-1
   EDID synopsis:
      Mfg id:           DEL
      Model:            DELL U3818DW
      Serial number:    97F8P91R0LFL
      Manufacture year: 2019
      EDID version:     1.3
   VCP version:         2.1

pi@raspberrypi:~ $ ddcutil detect --verbose
Output level:               Verbose
Reporting DDC data errors:  false
Trace groups active:        none
Traced functions:           none
Traced files:               none
Force I2C slave address:    false

Display 1
   I2C bus:             /dev/i2c-1
      I2C address 0x30 (EDID block#)  present: false
      I2C address 0x37 (DDC)          present: true 
      I2C address 0x50 (EDID)         present: true 
      /sys/bus/i2c/devices/i2c-1/name: bcm2835 (i2c@7e804000)
   EDID synopsis:
      Mfg id:           DEL
      Model:            DELL U3818DW
      Serial number:    97F8P91R0LFL
      Manufacture year: 2019
      EDID version:     1.3
      Product code:     41200
      Extra descriptor: Unspecified
      Video input definition: 0x80 - Digital Input
      Supported features:
         DPMS standby
         DPMS suspend
         DPMS active-off
         Digital display type: RGB 4:4:4
         Standard sRGB color space: False
      White x,y:        0.313, 0.329
      Red   x,y:        0.640, 0.330
      Green x,y:        0.300, 0.600
      Blue  x,y:        0.150, 0.060
      Extension blocks: 1
   EDID source: 
   EDID hex dump:
              +0          +4          +8          +c            0   4   8   c   
      +0000   00 ff ff ff ff ff ff 00 10 ac f0 a0 4c 46 4c 30   ............LFL0
      +0010   05 1d 01 03 80 58 25 78 ee ee 95 a3 54 4c 99 26   .....X%x....TL.&
      +0020   0f 50 54 a5 4b 00 71 4f 81 00 81 80 a9 40 d1 c0   .PT.K.qO.....@..
      +0030   01 01 01 01 01 01 4c 9a 00 a0 f0 40 2e 60 30 20   ......L....@.`0 
      +0040   3a 00 70 6f 31 00 00 1a 00 00 00 ff 00 39 37 46   :.po1........97F
      +0050   38 50 39 31 52 30 4c 46 4c 0a 00 00 00 fc 00 44   8P91R0LFL......D
      +0060   45 4c 4c 20 55 33 38 31 38 44 57 0a 00 00 00 fd   ELL U3818DW.....
      +0070   00 18 55 19 73 28 00 0a 20 20 20 20 20 20 01 3c   ..U.s(..      .<
   VCP version:         2.1
   Controller mfg:      RealTek
   Firmware version:    65.3
   Monitor returns DDC Null Response for unsupported features: false
```

```
VCP code 0x02 (New control value             ): One or more new control values have been saved (0x02)
VCP code 0x03 (Soft controls                 ): Button 2 active (sl=0x02)
VCP code 0x0b (Color temperature increment   ): 2 degree(s) Kelvin
VCP code 0x0c (Color temperature request     ): 3000 + 2 * (feature 0B color temp increment) degree(s) Kelvin
VCP code 0x0e (Clock                         ): current value =     2, max value =   255
VCP code 0x10 (Brightness                    ): current value =    73, max value =   100
VCP code 0x11 (Flesh tone enhancement        ): mh=0x00, ml=0x64, sh=0x00, sl=0x49
VCP code 0x12 (Contrast                      ): current value =    75, max value =   100
VCP code 0x13 (Backlight control             ): mh=0x00, ml=0x64, sh=0x00, sl=0x4b
VCP code 0x14 (Select color preset           ): 6500 K (sl=0x05)
VCP code 0x16 (Video gain: Red               ): current value =   100, max value =   100
VCP code 0x17 (User color vision compensation): current value =   100, max value =   100
VCP code 0x18 (Video gain: Green             ): current value =   100, max value =   100
VCP code 0x1a (Video gain: Blue              ): current value =   100, max value =   100
VCP code 0x1c (Focus                         ): current value =   100, max value =   100
VCP code 0x1e (Auto setup                    ): Auto setup not active (sl=0x00)
VCP code 0x1f (Auto color setup              ): Auto setup not active (sl=0x00)
VCP code 0x20 (Horizontal Position (Phase)   ): current value =     0, max value =     1
VCP code 0x22 (Horizontal Size               ): current value =     0, max value =     1
VCP code 0x24 (Horizontal Pincushion         ): current value =     0, max value =     1
VCP code 0x26 (Horizontal Pincushion Balance ): current value =     0, max value =     1
VCP code 0x28 (Horizontal Convergence R/B    ): current value =     0, max value =     1
VCP code 0x29 (Horizontal Convergence M/G    ): current value =     0, max value =     1
VCP code 0x2a (Horizontal Linearity          ): current value =     0, max value =     1
VCP code 0x2c (Horizontal Linearity Balance  ): current value =     0, max value =     1
VCP code 0x2e (Gray scale expansion          ): mh=0x00, ml=0x01, sh=0x00, sl=0x00
VCP code 0x30 (Vertical Position (Phase)     ): current value =     0, max value =     1
VCP code 0x32 (Vertical Size                 ): current value =     0, max value =     1
VCP code 0x34 (Vertical Pincushion           ): current value =     0, max value =     1
VCP code 0x36 (Vertical Pincushion Balance   ): current value =     0, max value =     1
VCP code 0x38 (Vertical Convergence R/B      ): current value =     0, max value =     1
VCP code 0x39 (Vertical Convergence M/G      ): current value =     0, max value =     1
VCP code 0x3a (Vertical Linearity            ): current value =     0, max value =     1
VCP code 0x3c (Vertical Linearity Balance    ): current value =     0, max value =     1
VCP code 0x3e (Clock phase                   ): current value =     0, max value =     1
VCP code 0x40 (Horizontal Parallelogram      ): current value =     0, max value =     1
VCP code 0x41 (Vertical Parallelogram        ): current value =     0, max value =     1
VCP code 0x42 (Horizontal Keystone           ): current value =     0, max value =     1
VCP code 0x43 (Vertical Keystone             ): current value =     0, max value =     1
VCP code 0x44 (Rotation                      ): current value =     0, max value =     1
VCP code 0x46 (Top Corner Flare              ): current value =     0, max value =     1
VCP code 0x48 (Top Corner Hook               ): current value =     0, max value =     1
VCP code 0x4a (Bottom Corner Flare           ): current value =     0, max value =     1
VCP code 0x4c (Bottom Corner Hook            ): current value =     0, max value =     1
VCP code 0x52 (Active control                ): Value: 0x60
VCP code 0x54 (Performance Preservation      ): mh=0x00, ml=0xff, sh=0x00, sl=0x60
VCP code 0x56 (Horizontal Moire              ): current value =    96, max value =   255
VCP code 0x58 (Vertical Moire                ): current value =    96, max value =   255
VCP code 0x59 (6 axis saturation: Red        ): current value =    96, max value =   255
VCP code 0x5a (6 axis saturation: Yellow     ): current value =    96, max value =   255
VCP code 0x5b (6 axis saturation: Green      ): current value =    96, max value =   255
VCP code 0x5c (6 axis saturation: Cyan       ): current value =    96, max value =   255
VCP code 0x5d (6 axis saturation: Blue       ): current value =    96, max value =   255
VCP code 0x5e (6 axis saturation: Magenta    ): current value =    96, max value =   255
VCP code 0x60 (Input Source                  ): DisplayPort-1 (sl=0x0f)
VCP code 0x62 (Audio speaker volume          ): current value =     0, max value =   100
VCP code 0x63 (Speaker Select                ): Front L/R (sl=0x00)
VCP code 0x64 (Audio: Microphone Volume      ): current value =     0, max value =   100
VCP code 0x66 (Ambient light sensor          ): Invalid value (sl=0x00)
VCP code 0x6b (Backlight Level: White        ): current value =     0, max value =   100
VCP code 0x6c (Video black level: Red        ): current value =     0, max value =   100
VCP code 0x6d (Backlight Level: Red          ): current value =     0, max value =   100
VCP code 0x6e (Video black level: Green      ): current value =     0, max value =   100
VCP code 0x6f (Backlight Level: Green        ): current value =     0, max value =   100
VCP code 0x70 (Video black level: Blue       ): current value =     0, max value =   100
VCP code 0x71 (Backlight Level: Blue         ): current value =     0, max value =   100
VCP code 0x72 (Gamma                         ): SL: 0x00 ,  SH: 0x00
VCP code 0x7a (Adjust Focal Plane            ): current value =     0, max value =   100
VCP code 0x7c (Adjust Zoom                   ): current value =     0, max value =   100
VCP code 0x7e (Trapezoid                     ): current value =     0, max value =   100
VCP code 0x82 (Horizontal Mirror (Flip)      ): Normal mode (sl=0x00)
VCP code 0x84 (Vertical Mirror (Flip)        ): Normal mode (sl=0x00)
VCP code 0x86 (Display Scaling               ): Invalid value (sl=0x00)
VCP code 0x87 (Sharpness                     ): current value =     0, max value =   100
VCP code 0x88 (Velocity Scan Modulation      ): current value =     0, max value =   100
VCP code 0x8a (Color Saturation              ): current value =     0, max value =   100
VCP code 0x8c (TV Sharpness                  ): current value =     0, max value =   100
VCP code 0x8d (Audio Mute                    ): Invalid value (sl=0x00)
VCP code 0x8e (TV Contrast                   ): current value =     0, max value =   100
VCP code 0x8f (Audio Treble                  ): current value =     0, max value =   100
VCP code 0x90 (Hue                           ): current value =     0, max value =   100
VCP code 0x91 (Audio Bass                    ): current value =     0, max value =   100
VCP code 0x92 (TV Black level/Luminesence    ): current value =     0, max value =   100
VCP code 0x93 (Audio Balance L/R             ): current value =     0, max value =   100
VCP code 0x94 (Audio Processor Mode          ): Speaker off/Audio not supported (sl=0x00)
VCP code 0x95 (Window Position(TL_X)         ): current value =     0, max value =   100
VCP code 0x96 (Window Position(TL_Y)         ): current value =     0, max value =   100
VCP code 0x97 (Window Position(BR_X)         ): current value =     0, max value =   100
VCP code 0x98 (Window Position(BR_Y)         ): current value =     0, max value =   100
VCP code 0x99 (Window control on/off         ): No effect (sl=0x00)
VCP code 0x9a (Window background             ): current value =     0, max value =   100
VCP code 0x9b (6 axis hue control: Red       ): current value =     0, max value =   100
VCP code 0x9c (6 axis hue control: Yellow    ): current value =     0, max value =   100
VCP code 0x9d (6 axis hue control: Green     ): current value =     0, max value =   100
VCP code 0x9e (6 axis hue control: Cyan      ): current value =     0, max value =   100
VCP code 0x9f (6 axis hue control: Blue      ): current value =     0, max value =   100
VCP code 0xa0 (6 axis hue control: Magenta   ): current value =     0, max value =   100
VCP code 0xa4 (Turn the selected window operation on/off): SL: 0x00 
VCP code 0xa5 (Change the selected window    ): Full display image area selected except active windows (sl=0x00)
VCP code 0xaa (Screen Orientation            ): Invalid value (sl=0x00)
VCP code 0xac (Horizontal frequency          ): 33264 hz
VCP code 0xae (Vertical frequency            ): 60.00 hz
VCP code 0xb2 (Flat panel sub-pixel layout   ): Red/Green/Blue vertical stripe (sl=0x01)
VCP code 0xb4 (Source Timing Mode            ): mh=0x00, ml=0x01, sh=0x00, sl=0x01
VCP code 0xb6 (Display technology type       ): LCD (active matrix) (sl=0x03)
VCP code 0xb7 (Monitor status                ): Value: 0x03
VCP code 0xb8 (Packet count                  ):     3 (0x0003)
VCP code 0xb9 (Monitor X origin              ):     3 (0x0003)
VCP code 0xba (Monitor Y origin              ):     3 (0x0003)
VCP code 0xbb (Header error count            ):     3 (0x0003)
VCP code 0xbc (Body CRC error count          ):     3 (0x0003)
VCP code 0xbd (Client ID                     ):     3 (0x0003)
VCP code 0xbe (Link control                  ): Link shutdown is enabled (0x03)
VCP code 0xc0 (Display usage time            ): Usage time (hours) = 4667 (0x00123b) mh=0xff, ml=0xff, sh=0x12, sl=0x3b
VCP code 0xc2 (Display descriptor length     ): current value =  4667, max value = 65535
VCP code 0xc4 (Enable display of 'display descriptor'): mh=0xff, ml=0xff, sh=0x12,
VCP code 0xc6 (Application enable key        ): 0x45cc
VCP code 0xc8 (Display controller type       ): Mfg: RealTek (sl=0x09), controller number: mh=0x00, ml=0x11, sh=0x11
VCP code 0xc9 (Display firmware level        ): 65.3
VCP code 0xca (OSD                           ): OSD Disabled (sl=0x01)
VCP code 0xcc (OSD Language                  ): English (sl=0x02)
VCP code 0xcd (Status Indicators             ): SL: 0x02 ,  SH: 0x00
VCP code 0xce (Auxiliary display size        ): Rows=0, characters/row=2 (sl=0x02)
VCP code 0xd0 (Output select                 ): Analog video (R/G/B) 2 (sl=0x02)
VCP code 0xd4 (Stereo video mode             ): Value: 0x02
VCP code 0xd6 (Power mode                    ): DPM: On,  DPMS: Off (sl=0x01)
VCP code 0xd7 (Auxiliary power output        ): Disable auxiliary power (sl=0x01)
VCP code 0xda (Scan mode                     ): Underscan (sl=0x01)
VCP code 0xdb (Image Mode                    ): Full mode (sl=0x01)
VCP code 0xdc (Display Mode                  ): Standard/Default mode (sl=0x00)
VCP code 0xde (Scratch Pad                   ): SL: 0x00 ,  SH: 0x00
VCP code 0xdf (VCP Version                   ): 2.1

```