# Nordic (nRF52) MCU Support

This package contains our support for writing nRF52 series MCU applications optionally with wireless radio support. Note that we we use a custom USB/DFU bootloader and flash memory layout compared to the standard nRF libraries.

This is currently the most mature and recommended MCU platform for building most general applications and anything needing wireless connectivity. Since nRF chips have a 64 Mhz 32-bit ARM, this includes basically all Arduino style usecases though compute intensive applications may need to use an RP2/STM32 or Raspberry Pi based solution.

## Getting Started

To start using an nRF board, start by following the [flashing](/pkg/peripherals/doc/flashing.md) instructions to program the bootloader for the board. After this, all future programming is done over USB.

If you have a nRF52840 USB Dongle, you can compile and flash a basic program to blink the LEDs as follows: 

```
cargo run --bin builder -- \
    build //pkg/nordic:nordic_blink --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- \
    built/pkg/nordic/nordic_blink uf2-dfu --usb_device_id=8888:0001
```

## Design Details

### Hardware Assumptions

When designing a new nRF52 board in Kicad, keep the following best practices in mind:

- Use a 3.3V VDD and rely on an external 3.3V regulator if significant (>10mA) of power is needed for connected devices.
- Connecting the 32kHZ crystal oscillator is optional but recommended.
- Internal DC/DC regulators in the nRF52 are disabled by default so no inductors on needed on the DDC/DEC pins.
- Expose a TC2030 pad if possible following the pinout in the [flashing guide](/pkg/peripherals/doc/flashing.md).
    - Minimally expose VCC/SWDIO/SWCLK/GND.
- DO NOT use the RESET pin as a GPIO pin (keep it as a reset pin with a push button exposed if possible)

TODO: Document recommended nRF modules.

### Flash Memory Layout

The internal MCU flash memory uses the following layout (for 1 MiB nRF52840 with 4 KiB pages as an example):

- `0x00000000 - 0x00007000` (28 KiB / 7 pages) : Bootloader Code
- `0x00007000 - 0x00008000` (4 KiB / 1 page) : Bootloader Params
    - These are erased and rewritten on each re-flash of the application/bootloader.
    - More specifically the format of this page is:
        - `[CRC32]` : 4 bytes. Computed over the `DATA` field.
        - `[LENGTH]` : 4 bytes. Length of the `DATA` field.
        - `[DATA]` : Variable length serialized `BootloaderParams` proto.
- `0x00008000 - 0x000F0000` : Application Code
- `0x000F0000 - 0x00100000` (64 KiB / 16 pages) : Application Params
    - Wear leveled storage of key/value params (where each key is a 32-bit id hardcoded in the firmware).
    - See the `nordic::ParamsStorage` struct for usage.
    - The format design is explained in the [parameter storage doc](//pkg/peripherals/doc/params.md).

### Bootloader

The custom bootloader is implemented in the `nordic_bootloader` binary. It is intentionally very simple and only supports DFU over USB based updating. Unlike normal applications, the bootloader copies its code into RAM before starting to run. This enables it to re-flash itself if needed.

Normally if a valid application is present, the bootloader will jump directly to it without delay. The bootloader can be made to indefinitely wait for flashing commands over USB if one of the following happens:

1. The MCU was reset via the RESET pin.
2. There was an application requested reset via `reset_to_bootloader()`.
    - The standard USB listener will react to DFU requests received in application mode by calling this.

The entire boot sequence is described below:

- MCU starts up or resets with the interrupt table located at memory/flash address 0x00000000.
- MCU jumps to the bootloader `entry()` function which is pointed to by the value at memory address 0x00000000.
- All the bootloader code, and RAM variables are copied from flash into RAM.
- Execution jumps to `main()` which is now located in RAM.
- APPROTECT if needed for newer ICs.
- The `BootloaderParams` proto is read from flash.
- If the application matches the stored CRC in the param and no 'reset to bootloader' condition was detected, then the application will be run.
- Else, the bootloader will run a USB controller to respond to DFU commands.
- Once a host finishes flashing an application, the bootloader will reset itself and attempt to execute the application.

### Radio Protocol

This section describes the wireless/radio protocol we use for communication between nodes. The general goals of this protocol are:

- All traffic is encrypted and authenticated with replay protection.
- No complex error prone pairing process.
    - Devices must be configurable once and run for years.
    - Packet loss should be acceptable.
- Hub centric model.
    - We aim to design around having 1-N central always on hub MCUs (e.g. connected to Raspberry Pis) that are handle translation from IP network received RPCs and are always ready to ingest spurious packets from things like sensors.
- Expose a basic lossy stream of data chunks to applications.
    - It is the application's responsibility to decide if it needs to implement ACKing, multi-packet chaining for larger payloads, or retrying.
    - For most applications, it is generally advisable to keep packets small and as idempotent as possible.

Quick overview of technical specs:

- 1 packet can have up to 246 bytes of application data and is transmitted at ~2 Mbps.

Noteable limitations:

- No hardening against physical hardware attacks
    - Lifetime AES keys are stored in FLASH and aren't refreshed via handshakes so if an attacker gets physical access to a device they can steal the key and decrypt any previously sniffed traffic going to the device (note that each device gets its own key so usually this would only compromise one device).

#### Pre-negotiation

We assume that we have a local network cluster and database set up. When we want to set up / 'pair' a new device, we connect a device to a PC via USB and run a program that does do the following:

- Generate a random 32-bit address for the device.
- For every other device we want it to communicate with, we generate a:
    - `key` : Random 16-byte secret AES key
    - `iv` : Random secret 5 byte string used for AES-CCM encryption
    - Index of device 'A' in device 'B's address book and vice versa (0-128 value using the next unused slot).
        - These are effectively used to save 3 bytes over the wire by sending indexes instead of full addresses when communicating.  
- Typically the only other device a device will talk to is a hub radio connected to a Raspberry Pi or similar computer.
- All the generated values including the address of both involved devices are stored in local memory of each involved device:
    - In FLASH memory for non-hub devices (transfered over USB).
    - In the compute cluster's database for hub devices.
- Later each device can communicate indefinitely with any device in its local 'address book' using all the above information.

#### Packet/Wire Format 

Over the air we use the Nordic Proprietary 2 Mbps sending on a static radio channel frequency with each packet having the following format:

- `[PREAMBLE]` : 1 byte. Standard for NRF 2Mbit protocol
- `[TO_ADDRESS]`: 4 bytes. Who we are sending the packet to.
- `[S0]` : 0 bytes
- `[LENGTH: 1 byte]`
- `[S1]` : 0 bytes
- `[PAYLOAD]` : `LENGTH + STATLEN` bytes (where `STATLEN` is 2)
    - `[FROM_ADDRESS]` : 4 bytes. Address of the 
    - TODO: Replace the above with this field:
        - `[FROM_ADDRESS_INDEX]` : 1 byte. Prenegotiated index of the device that sent the packet in the recipient's 'address book'.
            - TODO: Support excluding this if the recipient only has 1 connected device.
    - `[COUNTER]` : 4 bytes : Monotonically increasing packet counter used mainly for encryption and deduplication purposes.
    - `[CIPHERTEXT]` : Up to 246 bytes. Encrypted application data.
    - `[MIC]` : 4 bytes. AES-CCM MIC.
- `[CRC]` : 2 bytes
  - Uses IEEE 802.15.4 standard of CRC-16 starting with the first byte after the length.

In RAM, the packet buffer is stored as `[S0, LENGTH, S1, PAYLOAD]` and is restricted to be up to 258 bytes in length.

Note that unlike Bluetooth, the packet counter is embedded in each packet so a significant number of packets can be lost without compromising the underlying connection.

#### Encryption

Encryption is done using AES-CCM using a 2 byte message length, 13 byte nonce, and 4 byte tag/MIC.

The nonce is formed as follows:

- 4 byte packet counter
- 4 byte 'from address'
    - This is needed since we share the same AES key/IV in both transfer directions so extra differtiation is needed to allow both devices to overlap in packet counter values.  
- 5 byte pre-shared IV

Once the packet counter hits 2^32 (which shouldn't happen for a very long time), the device will stop working and will need to be re-programmed/negotiated with new keys.

To prevent against replay, every device should periodically store:

- Last received packet counter
    - Saved every 1 minute
    - TODO: Check how bad this is for flash durability.
- Last sent packet counter (+ N)
    - To avoid continuously writing to flash, what is actually stored is the 'last counter + 1000' and only rewritten when we try to send a packet with a larger counter. This means that device resets will have counter gaps but will never resent counters.

Devices will reject any packet received with a smaller counter value for a given link between 2 devices than was previously received. This protects against replays but an attacker may still potentially jam some packets and send them at a later time. If this is a concern for an application, it should implement some form of periodic heart beating between devices.

#### Traffic Control

Individual applications may choose how to prevent overlapping transmissions from different devices. A minimal implementation should at use the energy detection feature on the nRF radios and/or randomized backoff to retry unacknowledged packets.

