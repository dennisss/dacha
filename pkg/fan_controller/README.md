
Build using:
- `RUSTFLAGS="--emit asm" cargo build --release`
- `vim target/atmega32u4/release/deps/fan_controller-e011d0d02054b1c0.s`
- `# cargo build -Z build-std=core --target avr-atmega328p.json --release`

- `avrdude -v -patmega32u4 -P/dev/ttyACM0 -b57600 -cavr109 -D -Uflash:w:target/atmega32u4/release/fan_controller.elf:e`

- `avrdude -v -patmega32u4 -P/dev/ttyACM0 -b57600 -cavr109`

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


Thermistor Rank: 25C to 70C
- 10K to 2.3K

- Really need to go down to 20C (12.10K)
- 

- With 10K resistor
    - 2.5V -> 4.065V => dV = 1.56500
    - 2.262 => 1.80500
    - 3.165 => 0.9
- With 20K resistor
    - 3.333 ->  4.484 => dV = 1.15400
- With 5K resistor
    - 1.667 -> 3.425 => dV = 1.75800
    - 1.462 => 1.96300
    - 2.315 => 1.11
- With 2K
    - 0.833 -> 2.326 => dV = 1.49300
    - 

0.709 -> 2.326