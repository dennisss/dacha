# Uplift Desk Protocol

Desk takes as input a:
- RJ45 cable going to the Keypad
- RJ11 going to a bluetooth dongle

The bluetooth dongle is:
    - Marked as JCP35N-BLT-v3 on the PCB
    - Uses a CC2541 chip.

## Serial Protocol

This protocol is transmitted over the 4 middle pins of the RJ11 cable. On the dongle they are named:
- 5V
- MR: Sends bits from the dongle to the desk
- MT: Sends bits from the desk to the dongle.
- GND

RJ11 Cable colors:
- GND: Yellow
- 5V: Red
- MT: Green
- MR: Black

Serial config:
- 5V Logic
    - More specifically 4.8V high and 0.5V low
    - NOTE that the above low voltage is pretty high and may break 1.8V systems. Most likely you want to use at least a 3.3V VDD for interpreting these values (after level shifting).

    - TODO: Check if pulled up 
- Baudrate: 9600
- 8N1

### Packet Structure

Every packet send to/from either endpoint has the following format:

- Byte 0: Target Address
- Byte 1: Target Address (duplicate of byte 0)
- Byte 2: Command
- Byte 3: Payload length (N)
- Byte 4 to (4 + N): Payload
- Byte 4 + N: Checksum
    - Sum of all bytes from [1, 4 + N)
- Final Byte: 0x7E constant

### Address

- 0xF1: The desk control board (send me)
- 0xF2: The bluetooth dongle (receive me)

## Commands

### Current Height

Sent from the desk to the dongle to indicate the current position

- Command = 0x01
- Payload Length = 3
- Payload Contents:
    - First 2 bytes: Big endian height in units of 0.1 inches
    - Last byte: Always 0x07?
- Example Packet: [ 0xF2, 0xF2, 0x01, 0x03, 0x01, 56, 0x07, 68 ]
    - This means 1 * 265 + 56 tenths of an inch or '31.2 inches' overall.

### Move Up

Sent from dongle to desk to move the desk up by 1.4 inches.

- Command = 0x01
- Payload Length = 0
- Example Packet: [ 0xF1, 0xF1, 0x01, 0x00, 0x01, 0x7E ]

### Move Down

Sent from dongle to desk to move the desk down by 1.4 inches.

- Command = 0x02
- Payload Length = 0
- Example Packet: [ 0xF1, 0xF1, 0x02, 0x00, 0x02, 0x7E ]

### Set Key 1

Sent from dongle to desk to set the preset height for key 1 to the current height:

- Command = 0x03
- Payload Length = 0
- Example Packet: [ 0xF1, 0xF1, 0x03, 0x00, 0x03, 0x7E ]
- Once run, the keypad display will say: 'S -1'

### Set Key 2

- Command = 0x04
- Payload Length = 0
- Example Packet: [ 0xF1, 0xF1, 0x04, 0x00, 0x04, 0x7E ]
- Once run, the keypad display will say: 'S -2'

### Press Key 1

Sent from dongle to desk to trigger us to move to preset key 1's value.

- Command = 0x05
- Payload = 0
- Example Packet: [ 0xF1, 0xF1, 0x05, 0x00, 0x05, 0x7E ]

### Press Key 2

- Command = 0x06

### Set Key 3

- Command = 0x25

### Set Key 4

- Command = 0x26


### Unknown Command #8

Dongle -> Desk

This seems to always be the first packet sent by the dongle to the desk upon connecting.

- Command = 0x08
- Payload Length = 0
- Response to this seems to always be a packet of the form:
    - 242
    - 242
    - 5
    - 2
    - 255
    - 255
    - 5
    - 126

### Unknown Command #12

Dongle -> Desk

This seems to always be the second packet sent by the dongle to the desk upon connecting.


- Command = 0x0C
- Payload Length = 0
- Response to this seems to always be:
    - 242
    - 242
    - 7
    - 4
    - 5
    - 13
    - 2
    - 131
    - 162
    - 126

### Unknown Command #7

Dongle -> Desk

This seems to always be the third packet sent by the dongle to the desk upon connecting.

This command is useful for forcing the desk to respond with the current height if you don't currently know it.

- Command = 0x07
- Payload Length = 0
- Response:
    - Triggers a lot of different packets to be sent back by the desk
    - One of them will be a 'Current Height (0x01)' packet.
    - Rest have command ids from 0x25 to 0x28.

