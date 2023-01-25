# PC Fan Controller

This is a desktop computer fan controller.

## Requirements

There are the requirements we considered when designing a solution:

- Must fit in the back side of an NCase M1
    - There is a convenient slot between the side pannel latches where the fan controller can be stored.
    - This slot is 90mm wide.
- Flexibility for either 12V or 5V fans
    - Either all 12V or all 5V. Normally in a computer only 12V fans will be used though.
    - We will implement this by keeping the logic and fan power
- Support for 4-pin PWM fans
    - Need at least 4 PWM outputs for fans on 2x240mm radiators.
    - Need at least 1 PWM output for a water pump.
- Support for independent fan PWM output
    - Technical Specification:
        - See also https://noctua.at/pub/media/wysiwyg/Noctua_PWM_specifications_white_paper.pdf
        - Frequency: 25kHz varied from 0-100% duty cycle.
        - Internally pulled up by fans to 3.3/5V
- Support for reading tachometer input for failure detection
    - Must support independently checking each fan.
    - Must support reading a relatively low frequency as the fan pump
    - Technical Specification:
        - Fan exposes an open collector output (we must pull it up to VCC and fan will drive it to GND in pulses).
        - Fans emit 2 pulses per rotation
        - For 1500 RPM fans, need to support reading a 50Hz input wave
            - Should support between 5Hz and 150Hz for robustness/compatibility
- Power Requirements
    - Pass through 12V power for up to 6 fans. 
    - Expect 0.05A (0.6W) per fan
    - So 0.3A total
- Support for external control of the computer
    - Should be able to run logic MCU off of an external interface (e.g. USB).
    - Support detecting whether or not the computer is on (see if 12V power was applied)
    - Support for electronically pressing the motherboard Power/Reset buttons.
    - Support for modifying fan curves or reading out fan controller state over USB.
- Support SEN-FM18T10
    - https://koolance.com/coolant-flow-meter-stainless-steel-with-temperature-sensor-sen-fm18t10
    - 10K thermistor
    - Flow rate frequency input is 5Hz - 32Hz
- Support giving feedback to the motherboard as to whether or not the fan controller is working.
    - e.g. feed the CPU Fan header on the motherboard with a fan tachometer input so that the motherboard believes the fan is spinning.


## Hardware Design

### R3

This board has:

- RP2040 for all control logic.
- 6 4-pin PWM fan/pump inputs
    - 1 may be switched into a 'Fake CPU Fan'.
    - Spaced 0.5in apart.
- 2 10K thermistor inputs
- 1 water flow meter tachometer input.
- 1 USB-C control interface
- 1 4-pin PC power input port for attaching a Molex 12V/5V/GND connection.

#### PWM/Tachometer Output/Input Design

The RP2040 has 8 PWM slices each with 2 channels A/B. There are a few limitations to keep in mind:

- A single PWM slice can run at a single frequency at a time.
    - So if want to support a 'Fake CPU Fan' running at a lower frequency than other PWMs, it must be on a dedicated slice.
- The 'B' channel can be used as an input for frequency measurement (for analyzing the fan tachometer input).
    - But if 'B' is used as an input to the channel, the channel can't be used for anything else.
    - But, multiple GPIOs are mapped to the A and B channel of each slice so given that we don't need to always measure every single fan's speed, we can multiplex which fans we are measuring (up to 2 different pins' frequencies can be measured with one slice). 
- The RP2040 has a pull up/down resistance of ~50K so can be used to pull up the tachometer inputs.

So this leads us to the following mapping of slices to functions:
- Slices 5,6,7 will always run at 25kHz with both A/B channels usable as fan pwm outputs at different duty cycles (up to 6 fans).
- Slice 4 is dedicated to operate the 'Fake CPU Fan' with the channel operating at either 25kHz for PWM output or a lower frequency for tachometer output.
- Slices 0,1,2,3 will have up to 2 B channel pins connected each to enable measuring the frequency of up to 4 tachometer frequencies at once.


#### Thermistor Inputs

We aim to optimize from the temperature range 25C to 70C

- This results in a resistance range of 10K to 2.3K
- To be more flexible, we will support down to 20C (12.10K)

To select the second half of the voltage divider, we optimize for the widest voltage range below 3.3V each a 3.3V input.

- With 10K resistor
    - 1.493 V to 2.683 V = 1.19V range
- With 20K resistor
    - 2.056 V to 2.96 V = 0.904 V range
- With 5.6K resistor
    - 1.044 V -> 2.339 V = **1.294 V range**

#### Parts

- W25Q128JVSIM
    - https://www.digikey.com/en/products/detail/winbond-electronics/W25Q128JVSIM/6819721
    - 8 SOIC
- 12Mhhz crystal
    - 18pF
    - 0.126" L x 0.098" W (3.20mm x 2.50mm)
    - https://www.digikey.com/en/products/detail/cts-frequency-controls/403I35D12M00000/2636724
- 27pF 0603 caps

### R4

TODO

- Use a pair of TS3A5017 to be able to support 16 tachometer inputs and thus 16 fans.
- Further space saving can be be done with 0402 components.





## Old 2


- Tiny 2040


Requirements:
- When just 12V is connected, it must work
- When just USB is connected, it should work (to power it)
- Should protect power from flowing back down the USB 


Minimum Connection Requirement:
- 1 x Fans 1/2 PWM (A)
- 1 x Fans 3/4 PWM (B)
- 4 x Fan Speed Input (could be muxed)
- 1 x Pump PWM
- 1 x Pump Speed INput

- 1 x PC Power
- 1 x PC Reset

- 1 x Flow Speed input
- 2 x Temperature inputs

- Do I want to 


## Old


- Power inputs:
    - 5V from FPanel connector
        - Alternatives:
            - Motherboard USB or SATA
    - 12V
        - From CPU fan connector
        - Could also use this to determine if the computer is on?
- If we default to an internal connection, then we can use 
- Do I need a switchable power?

- ESP8266 will 



Wireless boards:
- nRF52832 can do zigbee

- Or use ESP8268 / ESP8285
- ESP32 is more capable but also bigger.
- nRF52840 is reasonable but I can't solver it down.

https://raw.githubusercontent.com/sparkfun/Arduino_Boards/master/IDE_Board_Manager/package_sparkfun_index.json

AVR Bugs
- https://github.com/rust-lang/compiler-builtins/issues/400
- https://gitter.im/avr-rust/Lobby?at=5ef3f129613d3b3394080eae
- https://reviews.llvm.org/D87631
    - Fixes the issue with the VTable.

- https://reviews.llvm.org/D82536
    - Fixes something else?

`rustup override set nightly-2021-01-07`

Instructions for patching and building custom rust stage 2 toolchain:
- https://objectdisoriented.evokewonder.com/posts/patching-llvm/

Build using:
- `RUSTFLAGS="--emit asm" cargo build --release`
- `vim target/atmega32u4/release/deps/fan_controller-e011d0d02054b1c0.s`
- `# cargo build -Z build-std=core --target avr-atmega328p.json --release`

- `avrdude -v -patmega32u4 -P/dev/ttyACM0 -b57600 -cavr109 -D -Uflash:w:target/atmega32u4/release/fan_controller.elf:e`

- `avrdude -v -patmega32u4 -P/dev/ttyACM0 -b57600 -cavr109`

- `avr-size --mcu=atmega32u4 --format=avr target/atmega32u4/release/fan_controller.elf`

Testing:

- `RUST_BACKTRACE=1 cargo test -- --test-threads=1 --nocapture`


Brand new 32u4 comes with DFU bootloader:
- http://ww1.microchip.com/downloads/en/DeviceDoc/doc7618.pdf

Prot Micro uses the Caterina Bootloader:
- AVR109/AVR910 protocol: http://ww1.microchip.com/downloads/en/AppNotes/doc1644.pdf

stty -F /dev/ttyACM0 speed 1200
stty -F /dev/ttyACM0 speed 57600
/home/dennis/.arduino15/packages/arduino/tools/avrdude/6.3.0-arduino14/bin/avrdude -C/home/dennis/.arduino15/packages/arduino/tools/avrdude/6.3.0-arduino14/etc/avrdude.conf -v -patmega32u4 -cavr109 -P/dev/ttyACM0 -b57600 -D -Uflash:w:/tmp/arduino_build_764244/Blink.ino.hex:i



`/home/dennis/.arduino15/packages/arduino/tools/avrdude/6.3.0-arduino14/bin/avrdude -C/home/dennis/.arduino15/packages/arduino/tools/avrdude/6.3.0-arduino14/etc/avrdude.conf -v -patmega32u4 -cavr109 -P/dev/ttyACM0 -b57600 -D -Uflash:w:/tmp/arduino_build_764244/Blink.ino.hex:i `

Arduino IDE measures memory usage like:

```
Sketch uses 4142 bytes (14%) of program storage space. Maximum is 28672 bytes.
Global variables use 149 bytes (5%) of dynamic memory, leaving 2411 bytes for local variables. Maximum is 2560 bytes.
```

How to get 25kHz PWM:
- https://arduino.stackexchange.com/a/25623


## Features

Ports
- 3 controllable PWM fan/pump channels.
- 5 fan/pump inputs.
- 2 10K temperature sensor inputs
- 1 water flow sensor input
- 1 motherboard feedback port
- 1 motherboard front panel control port
- 1 USB

Use-cases



## Software Design


The flexible one:
- EK-STC Classic 10/13 - Nickel	
- EK-DuraClear 9,5/12,7mm 3M RETAIL	

New ZMT:
- EK-Tube ZMT Matte Black 15,9/9,5mm
- EK-STC 10/16mm

Plasticizer issues:
- https://www.ekwb.com/blog/what-is-plasticizer/

- Note: Will need a cyclic output buffer.

Protocol:
- Byte 0: CRC8 of this packet
- Byte 1: Message Type
- Byte 2: Sequence number (up to 128) - Top bit means if this is the last packet
- Byte 3: Length of this packet.
- Bytes 4-63: Up to 60 bytes of payload
    - Mainly limited to 64 bytes by the USB driver
- We will also have a special ACK and FAIL message types.


Main thread:
- Mark start time and get initial rotation counter values
- Wait 1 second
- Mark stop time and get final rotation counter values
- Read digital pin connected to Power LED
- Run ADC on temps (could probably be done during the sleep interval)
- Run Pulse sampling on the CPU input
- Run fan speed calculation
- Update PWM outputs (reconfigure timers)
- Mark that data is available to send back to the companion computer

USB/SPI Threads
- Wait for either:
    - (prioritize) Notification of new measurement
    - Remote byte received
- If new measuremnt available
    - Copy to output buffer
- If remote byte received
    - Add to 

Output Thread
- Basically continously pushes data in single packet internals into the output queue

Counter Threads:
- One thread per fan speed input: basically this just increments a u16 value (allowing for wrapping)

## Revision 2 Thoughts

- Better support for having more PWMs or configurability of which inputs are used
- Expose either the Slave Select to allow SPI transfers or expose the UART pins
- Make the auxiliary power pins larger (maybe big enough to support a screw terminal)
- Flip the 4-pan fan headers upside down
- add diagonal drill holes

- Consider using WiFi via an ESP8266 port
- If I can mount a 

- Put the power LED on an interrupt pin that can wake up the MCU from sleep.

## Pinout

PB0 - WATER_FLOW : INPUT PCINT0
PB1 - ISP_SCK
PB2 - ISP_MOSI
PB3 - ISP_MISO
PB4 - CPU_PWM_IN : INPUT (sample duty cycle with digital reads)
PB5 - FAN_PWM_C : OUTPUT OC1A
PB6 - FAN_PWM_B : OUTPUT OC1B
PB7 - CPU_SPEED_OUT - OUTPUT OC0A regular 20Hz PWM wave (or we could use OC1C to have a 4th Fan pwm output)

PC6 - FAN_PWM_A  : OUTPUT OC3A
PC7 - LED (Active Low)

PD0 - FAN_SPEED_4 : INPUT_PULLUP INT0
PD1 - FAN_SPEED_3 : INPUT_PULLUP INT1
PD2 - FAN_SPEED_2 : INPUT_PULLUP INT2
PD3 - FAN_SPEED_1 : INPUT_PULLUP INT3
PD4 - LED (Active Low) : OUTPUT
PD5 - FPANEL_PLED : INPUT
PD6 - FPANEL_POWER : OUTPUT
PD7 - FPANEL_RESET : OUTPUT

PE2: N/C High-Z
PE6: FAN_SPEED_5 : INPUT_PULLUP INT6

PF0 - WATER_TEMP : INPUT ADC0
PF1 - AIR_TEMP   : INPUT ADC1
PF4 - ENABLE_TEMP: OUTPUT (Active High)
PF5 - LED (Active Low) : OUTPUT
PF6 - LED (Active Low) : OUTPUT
PF7 - LED (Active Low) : OUTPUT

## Old


Requirements:
- Inputs:
    - Frequency Counter
        - 4 x fans
        - 1 x pump
        - 1 x flow rate sensor
        - (1 x extra fan)
    - Analog Voltage
        - 1 x water temperate
        - 1 x extra temperature point
- Outputs:
    - PWM @ 25kHz
        - 4 x fans
        - 1 x pump
    - USB to talk to computer (or Pi)
    - Power switch override

Voltages?
- Fans: 5V input and output

- Pull up the fan inputs (possibly using internal arduino functions)


With a Pi Zero?
- No analog inputs

Doing this with Atmega?
- Not enough interrupts
    - Need not know the speed of all motors at once.
    - If we can tell one fan and then switch to another, then that is ol
- Difficult to get PWM at 25kHz
    - 

- Useful link to reflash bootloader on Pro Micro
    - https://forum.arduino.cc/index.php?topic=363341.0

Pro Micro
- The Pro Micro has five external interrupts, which allow you to instantly trigger a function when a pin goes either high or low (or both). If you attach an interrupt to an interrupt-enabled pin, you'll need to know the specific interrupt that pin triggers: pin 3 maps to interrupt 0 (INT0), pin 2 is interrupt 1 (INT1), pin 0 is interrupt 2 (INT2), pin 1 is interrupt 3 (INT3), and pin 7 is interrupt 4 (INT6).

- Use Timer 1 to generate 2 25kHz waves
    - https://arduino.stackexchange.com/a/25623
    - Pins 9 and 10
    - Can get more if using a Mega

    - Timer 1 pins
        - PB6, Pb5
    - Timer 3 pins
        - PC6 (digital pin 5)

    - Ideally want at leat 3

- Use timer 4 to generate output PWM for CPU feedback


- so final pinout
    - Pin 0: Pulse Interrupt - PD2
    - Pin 1: Pulse Interrupt - PD3
    - Pin 2: Pulse Interrupt - PD1
    - Pin 3: Pulse Interrupt - PD0
    - Pin 5: PWM 25kHz
    - Pin 7: Pulse Interrupt - PE6
    - Pin 9: PWM 25kHz
    - Pin 10: PWM 25kHz
    - Pin 12 or 13: Feedback to motherboard for buzzer keep alive.
    - Pin 18: Analog In
    - Pin 19: Analog In
    - still need to assign pins for power/reset switch override and
    - Tap into one of the 5V molexes for main power and read if sleeping or not
    - Also add a buzzer!
        - Or use the PC buzzer by outputting an extra PWM signal
        - Also need a reset switch, programming header, etc.

- TODO: Need an interrupt for the flow sensor!

Crystal
- YSX321SL

- FPANEL_PLED should be pulled down to ground 

Need separate power lines:
- Need to be able to turn up computer while it is off.
    - So VUSB must feed the microcomtroller.
    - 

- NOTE: DFU does contain a USB bootloader
    - http://ww1.microchip.com/downloads/en/devicedoc/doc7618.pdf

- Programming Pins
    - MOSI
    - MISO
    - SCK
    - RESET
    - 5V + GND

- Pull up Puse interrupts with a 10k

- Remove antenna connector at rthe 

    - TODO: If building my own board, then I need to leave enough pins exposed to do a bootloader reprogramming.
        - Would also need fuse on USB input to avoid pulling power from there?

NOTE:

- Fan control algorithm
    - 
    - Inputs: Temperatures
    - 



Red:
- Vf = 2
- (1k)
Green
- Vf = 3.3  (forward = 25)
Blue
- Vf = ~2.8

1K Resistor
-  C21190

10K Resisror
-  C25804

560 resistor
- C23204






0.709 -> 2.326


USB Descriptors from the Pro Micro
==================================

CDC mode set rate to 1200hz to trigger a reset to bootloader.


  bLength                18
  bDescriptorType         1
  bcdUSB               2.00
  bDeviceClass          239 Miscellaneous Device
  bDeviceSubClass         2 
  bDeviceProtocol         1 Interface Association
  bMaxPacketSize0        64
  idVendor           0x1b4f 
  idProduct          0x9206 
  bcdDevice            1.00
  iManufacturer           1 
  iProduct                2 
  iSerial                 3 
  bNumConfigurations      1
  Configuration Descriptor:
    bLength                 9
    bDescriptorType         2
    wTotalLength       0x004b
    bNumInterfaces          2
    bConfigurationValue     1
    iConfiguration          0 
    bmAttributes         0xa0
      (Bus Powered)
      Remote Wakeup
    MaxPower              500mA
    Interface Association:
      bLength                 8
      bDescriptorType        11
      bFirstInterface         0
      bInterfaceCount         2
      bFunctionClass          2 Communications
      bFunctionSubClass       2 Abstract (modem)
      bFunctionProtocol       0 
      iFunction               0 
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        0
      bAlternateSetting       0
      bNumEndpoints           1
      bInterfaceClass         2 Communications
      bInterfaceSubClass      2 Abstract (modem)
      bInterfaceProtocol      0 
      iInterface              0 
      CDC Header:
        bcdCDC               1.10
      CDC Call Management:
        bmCapabilities       0x01
          call management
        bDataInterface          1
      CDC ACM:
        bmCapabilities       0x06
          sends break
          line coding and serial state
      CDC Union:
        bMasterInterface        0
        bSlaveInterface         1 
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x81  EP 1 IN
        bmAttributes            3
          Transfer Type            Interrupt
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0010  1x 16 bytes
        bInterval              64
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        1
      bAlternateSetting       0
      bNumEndpoints           2
      bInterfaceClass        10 CDC Data
      bInterfaceSubClass      0 
      bInterfaceProtocol      0 
      iInterface              0 
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x02  EP 2 OUT
        bmAttributes            2
          Transfer Type            Bulk
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0040  1x 64 bytes
        bInterval               0
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x83  EP 3 IN
        bmAttributes            2
          Transfer Type            Bulk
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0040  1x 64 bytes
        bInterval               0
