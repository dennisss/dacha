# Wireless "Buttons" 

TLDR: [Watch this video first](https://www.youtube.com/watch?v=ljrKFFjFT04)

This page and directory contains the source files and documentation for the wireless button/sensor projects specifically.

- `boards/` directory contains the electronics Kicad files.
- `parts/` directory contains the 3d printed parts.
    - Heatset inserts are M2 with M2 6mm screws used
- `src/` is the code that links 

Related pages:

- nRF libraries and wireless protocol documentation: [//pkg/nordic/index.md](/pkg/nordic/index.md)
- main file for the sensor firmware: [//pkg/nordic/src/bin/nordic_sensor.rs](/pkg/nordic/src/bin/nordic_sensor.rs)

## Commands

**Receiver Setup**

We will prepare the [nRF52 USB Dongle](https://www.digikey.com/en/products/detail/nordic-semiconductor-asa/NRF52840-DONGLE/9491124) to act as the receiver.

First, flash a bootloader to the board following [these flashing instructions](/pkg/peripherals/doc/flashing.md).

Then we can build and flash the board with the dongle firmware as follows:

```
cargo run --bin builder --  build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840
cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:0001
```

Then you can start the bridge program that will allow other programs to use the receiver:

```
cargo run --bin nordic_radio_bridge -- --state_object_name=nordic_radio_bridge_config --port=8002
```

- `state_object_name` is the key of a row stored in network wide storage (currently this requires that you first set up a cluster using [these instructions](/pkg/cluster/index.md)). 

TODO: Support 'state_object_name' being a local file.

**Button Board Flashing**

Flashing the bootloader to the button board (via SWD cable):

(See [this page](/pkg/peripherals/doc/flashing.md) for more information)

```
cargo run --bin builder -- build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader

cargo run --bin flasher -- built/pkg/nordic/nordic_bootloader blackmagic-swd --power_device
```

Flashing the button board's firmware (via USB using the bootloader):

```
cargo run --bin builder --     build //pkg/nordic:nordic_sensor --config=//pkg/nordic:nrf52840_soft

cargo run --bin flasher --     built/pkg/nordic/nordic_sensor uf2-dfu --usb_device_id=8888:0001
```

**Button/Sensor Setup**

This section will configure the button/sensor via USB so that it starts working and sending/receiving data. Important things to know:

- Devices have a human readable name e.g. 'btn1' which is what is used by the bridge and local programs to identify the device. Internally this is translated to a 32-bit address
- Configurations for sensors are stored in the [//pkg/nordic/config/sensors/](/pkg/nordic/config/sensors/) directory where the name of each file is the name of the config. You will want to use a different config depending on what type of device you are setting up.

To set up a device you can run a command like the following (requires the receiver bridge to be running):

```
cargo run --bin nordic_radio -- \
    setup_sensor \
    --name=btn1 --config_name=button --bridge_addr=localhost:8002
```

Note that after this step, the sensor board needs to be powered off and then powered back on to start working.

Deleting an existing device can be done as follows:

```
cargo run --bin nordic_radio -- \
    remove_device \
    --device_name=btn1 --bridge_addr=localhost:8002
```

**Basic Light Automation**

AS an example of how to use the button, you can set it up to control hue lights as follows:

First create a hue API key:

```
cargo run --bin hue -- create_user \
    --application_name=dacha --device_name=button
```

Store the user name in storage:

```
cargo run --bin cluster_cli -- set_object button_hue_key --value=[user_name]
```

Run a pipeline that will translate packets from a button into calls to the lights API:

```
cargo run --bin button -- \
    light-button \
    --bridge_addr=localhost:8002 \
    --button_device_name=btn1 \
    --hue_key_object=button_hue_key \
    --hue_group_name=Bedroom
```

**Misc Commands**

Debugging the current network/encryption config of a sensor:

```
cargo run --bin nordic_radio -- get_config --usb_device_id=8888:0006
```

Debugging the current sensor config of a USB connected sensor:

```
cargo run --bin nordic_radio -- get_sensor_config --usb_device_id=8888:0006
```

Listing all devices on the network:

```
cargo run --bin nordic_radio -- list_devices --bridge_addr=localhost:8002
```


## Components

### MCU

**nRF52840**

- [BMD-380-A-R](https://www.digikey.com/en/products/detail/u-blox/BMD-380-A-R/12759179)
    - 9.5 x 7.5 x 1.5 mm
    - $8.81000
    - Internal system power is 1.3V
    - Min VDD is 1.7V
    - Current Usage
        - Sleep mode (wake on any event): 2uA
        - Radio transmission: ~10mA
        - Should use the DCDC converter and external crystal
    - TODO: Need to attach an external crystal to this.

In order to run the nRF52 in the lowest power state on a custom board, you need to ensure that you are doing the following:

- Enabling the internal DC/DC converters
- Turn off the HLCLK and any other peripherals while sleeping / not in use (sleeping is when calling `WFI`). 
- Retain only the minimum amount of required RAM
- Wire up the VCC pin to the VCCH pin directly.
    - If you don't do this, the chip will consume extra power attempting to use the first regulator stage.
- Ideally wire up an external LFCLK crystal instead of using the internal one.
- Do not have any unused interrupts enabled (via INTENSET)
    - Even if you don't configure the NVIC to trigger an interrupt for them, if there is a pending event, the pending event/interrupt will consume noticeable power while polling to see if it should wake up the CPU.

**Possible Future UpgradeOption**

- 453-00223R
    - nRF54L15
    - 7.9mm x 6.3mm x 1.75mm
    - $4.40
    - No internal crystal
    - Not sure if it has DC/DC converter inductors?


### Battery

CR2032

- Up to 10 year shelf life
- 2.7 - 3.3V
- 220mA
- [Battery Holder](https://www.digikey.com/en/products/detail/mpd-memory-protection-devices/BK-912-TR/2077831)
    - [Cheaper version](https://www.digikey.com/en/products/detail/mpd-memory-protection-devices/BK-912/2647825)

Battery ESR is a concern and will lead to voltage drop:

- Use a 100uF 16V 1210 footprint ceramic cap
    - https://www.digikey.com/en/products/detail/taiyo-yuden/EMK325ABJ107MM-P/7067011
    - Ceramic caps have the lowest leakage.
        - But the issue with ceramics is DC bias so you need to use a large physical size / voltage rating. 
    - 100uF is probably overkill and has a small insulation resistance (will result in continous leakage current).
        - Lower capacitances (e.g. 47uF) are probably better and generally have higher insulation resistance.

### Expansion Headers

https://www.digikey.com/en/products/detail/harwin-inc/M50-3530442/7044015

- 1.27mm pitch
- 2.1mm wide (+/- 0.5mm)
    - CAD will use 2.15 as the width

Headers will be:

- X: 1.2mm away from edge
    - Min would be 1.075

- Y: 0.8mm away from edge
    - Min would be 1.27 / 2 = 0.635


### Sensors

#### Button 

https://www.digikey.com/en/products/detail/e-switch/TL9210AF260Q/4965616

#### Indicator LED

- Narrow FoV
- https://www.digikey.com/en/products/detail/lumex-opto-components-inc/SML-LXIL0603USBCTR/9866809
- Forward voltage is around 3V so we can probably just PWM it without a resistor.

#### Rotary Encoder

Most common size uses a ~6mm shaft/knob

Output signal we will support is a A/B quadrature encoded pulses.

Must get one that has same number of detents and pulses/rev and has a quadrature decode table that shows that both signals are off/high 

- https://www.digikey.com/en/products/detail/bourns-inc/PEC12R-4220F-S0024/4499653
    - 20mm shaft length
    - 24 detents
    - 24 pulses per 360 degrees

#### Contact Sensor

Want to detect presence of a magnet but we should prefer not to use a reed switch since is not as easily configurable for intensity and the mechanical nature of the switch means that they are known to be flaky/sticky.

- Fixed threshold: DRV5032
    - Interrupt output
- Configurable (I2C): TMAG5273 (slightly higher power usage)

DRV5032FBDBZR
- https://www.digikey.com/en/products/detail/texas-instruments/DRV5032FBDBZR/7173718
    - https://www.ti.com/lit/ds/symlink/drv5032.pdf
    - 
- 4.8mT threshold
- 5Hz sampling (0.54uA)
- Omnipolar
- Push-pull output


#### PIR

[EKMB1107112](https://www.digikey.com/en/products/detail/panasonic-electric-works/ekmb1107112/10222335)

- [Datasheet](https://industrial.panasonic.com/cdbs/www-data/pdf/EWA0000/bltn_eng_ekmb119111_ast-ind-247377.pdf)
    - 2.3 - 4V supply
    - Digital output
- Expensive but it has 1uA current consumption.
- Output signal will be noisy due to low current level.
    - Should have a 10-100nF cap near the PIR power pins.

#### Motion Sensor

Accelerometer: [ADXL362](https://www.digikey.com/en/products/detail/analog-devices-inc/ADXL362BCCZ-RL/3757930)

- [Datasheet](https://www.analog.com/media/en/technical-documentation/data-sheets/ADXL362.pdf)
- Supply: 1.6V - 3.5V
- Interface:
    - SPI + interrupt pin 
- Current
    - 10nA standby current
    - 270nA motion activated wake up mode

Usecases:

- Door knock sensing
- Laundry sensing

#### Temperature / Humidity

- [HDC2080](https://www.digikey.com/en/products/detail/texas-instruments/HDC2080DMBR/9692560)
    - [Datasheet](https://www.ti.com/lit/ds/symlink/hdc2080.pdf?HQS=dis-dk-null-digikeymode-dsf-pf-null-wwe&ts=1768886356052&ref_url=https%253A%252F%252Fwww.ti.com%252Fgeneral%252Fdocs%252Fsuppproductinfo.tsp%253FdistId%253D10%2526gotoUrl%253Dhttps%253A%252F%252Fwww.ti.com%252Flit%252Fgpn%252Fhdc2080)
    - I2C : 10-400kHz
    - 50nA sleep (0.05uA)
    - 300nA RH measurement
    - 550nA RH + temp measurement

Higher power option:

- HDC3020
    - I2C
    - 0.4uA sleep
    - 99uA active

#### e-ink Display

e-ink
- https://www.seeedstudio.com/2-13-Monochrome-ePaper-Display-with-122x250-Pixels-p-5778.html?srsltid=AfmBOoraMSPhGNno79Ro7MZsgKDXIecJgfcY73RDxPmlMWZv5HLKvrnG
- https://files.seeedstudio.com/wiki/Other_Display/213-epaper/GDEY0213B74.pdf
- 2.13" monochrome with 122x250
- Model: GDEY0213B74

Components:

- [Connector](https://www.digikey.com/en/products/detail/hirose-electric-co-ltd/FH34SRJ-24S-0-5SH-50/5132528)
- [Diodes](https://www.digikey.com/en/products/detail/vishay-general-semiconductor-diodes-division/MSS1P3L-M3-89A/2071752)
    - Smaller and higher spec than the MBR0530's recommended in the datasheet
- [Load Switch](https://www.digikey.com/en/products/detail/texas-instruments/TPS22922YZPR/2057677)
- [Inductor](https://www.digikey.com/en/products/detail/abracon-llc/ASPI-4030S-470M-T/4215207)


#### Ambient Light

- [TI OPT3004](https://www.digikey.com/en/products/detail/texas-instruments/OPT3004DNPR/9858337) (1.8uA)
    - I2C + interrupt
    - Supply: 1.6 - 3.6V
    - 0.4uA shutdown
    - 3.7uA active.

Alternative option:

- VEML7700
    - I2C
    - Supply: 2.5V - 3.6V
    - 0.5uA shutdown
    - 4uA for refresh every 4 seconds
    - https://www.vishay.com/docs/84286/veml7700.pdf



