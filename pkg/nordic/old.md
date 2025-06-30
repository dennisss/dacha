THIS PAGE IS DEPRECATED

# Nordic

Next steps:
- Implement 2.4Ghz hub
  - Needs a DB to store:
    - Local network address and counter
    - Remote network addresses and key pairs
    - For each remote address, a canonical name of that device
    - Will for now use a single EmbeddedDB replica
  - Implement pub/sub based service for sending/receiving data
  - Need an NRF binary that can be used as a hub:
    - First USB NRF must be bootstrapped with keys and addresses
- Name of the radio protocol
  - radio-frame
- End to end encryption while decryption of key strokes only occuring on the connected x86 computer.

## Building

Designing a configuration for a MCU/Board
- MCU Specifics
  - Instruction Set (compiler target): ARM or RISC or what?
  - Chip Pinout
  - Memory Layout (Flash, RAM totals)
  - In the case of RP2040, a BOOT2 blob to write
  - Maybe values of reigsters of REGOUT0 that should be set.
  - Register map (SVD)
- Board Specifics
  - For specific MCUs
    - Supports LFCLK
  - Custom pin selection constants
- Use-case Specifics
  - For bootloader vs application memory layout (and whether to run in RAM) 
  - Dirrent 

Design:
- Each board config is defined as a build rule which emits a binary proto with a BuildConfig
  - Using Any protos, we can arbitrarily extend it
  - Build configs are always generated using a default configuration as they must be built with something and the build script obviously needs to 
- Later, if the linker needs to generate something it should take as input the BuildConfig and pull out any of the protos that it needs from that.


- Define top level config objects
  - These need to be fed into the linker script generator.
  - Each board 




  

Some flashing requirements:
- Want to be able to easily find the build id of a flashed firmware (ideally without interacting with it).


`cargo build --package nordic --target thumbv7em-none-eabihf --release --no-default-features`

`da build //pkg/nordic:nordic --config=//pkg/nordic:nrf52840`

`openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program built/pkg/nordic/nordic verify" -c reset -c exit`

## Flashing

### Locally

This assumes you have locally connected an NRF52840 Dev Kit with the Segger J-Link

You'll need to get openocd from git head as the Ubuntu version doesn't include all needed NRF commands:

```
sudo apt install --reinstall ca-certificates
sudo apt install libtool

git clone git@github.com:openocd-org/openocd.git
cd openocd

./bootstrap
./configure
make
sudo make install
```

Then run

`openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program target/thumbv7em-none-eabihf/release/nordic verify" -c reset -c exit`

To debug, omit the `-c exit` and run:

- `~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb target/thumbv7em-none-eabihf/release/nordic`
- `target extended-remote localhost:3333`
- `monitor reset halt`
- More useful commands: https://openocd.org/doc/html/GDB-and-OpenOCD.html


### Via Raspberry Pi

Make sure openocd is installed similar to above.

```
sudo apt install libtool git pkg-config libftdi-dev libusb-1.0-0-dev build-essential
git clone https://github.com/openocd-org/openocd.git
cd openocd

./bootstrap
./configure --enable-sysfsgpio --enable-bcm2835gpio
make
```

- Follow Adafruit guide for adding Open OCD
  - https://learn.adafruit.com/programming-microcontrollers-using-openocd-on-raspberry-pi/overview
  - Pins 24 (SWDIO) and 25 (SWDCLK) for SWD

Create board file:

```
source [find interface/raspberrypi-native.cfg]
bcm2835gpio swd_nums 25 24
transport select swd
source [find target/nrf52.cfg]
```

^ May need to change to 'interface/raspberrypi2-native.cfg' depending on the pi.


```
cargo build --package nordic --target thumbv7em-none-eabihf --release --no-default-features
scp -i ~/.ssh/id_cluster target/thumbv7em-none-eabihf/release/nordic pi@10.1.0.88:~/binary
openocd -f nrf52_pi.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program /home/pi/binary verify" -c reset -c exit
```




## Design

### Development Gotchas

- If clearing an event register immediately before triggering a task that may generate that event, if the task finishes too soon, then the event may not be generated again (must wait at least 4 cycles before starting the task)

### Radio Protocol

Network protocol requirements:
- Must support sending data from nodes that are in-frequently powered on
- Must support encryption
- Must support reliable transfer/acknowledgement
  - e.g. If a sensor wants to occasionally report its status, it should be able to retry if no one received the response
  - Some acknowledgements can be implicit:
    - e.g. a coordinator may request current the current state of a sensor
      - The response to that message would the one that requires the acknowledgement
- Large payloads that don't fit in one message.
  - Could be for large commands or responses with telemetry data.
- Use cases:
  - Supporting streaming of log data back (needs to be distinguished from other types of data)
  - Lazy streams: Only send data on a stream if the other endpoint 
  - Send back a 'serial channel'

High level design:
- We assume that all pairs of communicating devices have shared their addresses/keys with each out of band.
- We will provide a network 'frame' abstraction which allows a use to send up to 1 KB (1024 bytes) as one atomic unit.
- Each frame can be optionally acknowledged by the recipient.
- No ordering information between frames is communicated by the protocol.
- To support multiple applications running on the same device, each frame will be sent with a 'channel'

Plaintext payload format:
- `[FLAGS]`: 1 byte
  - Bit 7: REPLY: Whether or not this packet contains a reply (either ACK or NACK) to a previously sent 
  - Bit 6: ACK: Whether or not we want the recipient to ACK this frame.
  - Bit 5: END: Whether or not this is the final packet in a frame.
  - Bits 0-2: SEQUENCE_NUM: Starting at 0, the sequence number of the current packet in a frame.
    - NOTE: All packets in a single frame will have sequential SEQUENCE_NUMs and sequential COUNTER values.
- If `REPLY` == false:
  - `[CHANNEL]`: 1 byte
  - `[DATA]`: N bytes (channel specific data).
- If `REPLY` == true:
  - The following data structure is repeated 1+ times:
    - `[CHANNEL]`: 1 byte
    - `[COUNTER]`: 4 bytes: COUNTER number of the first packet in the frame being (N)ACK'ed
      - NACK is useful in the case that a receiver gets a partial 


- On the keyboard screen we can also monitor signal quality based on number of retries.

Keyboard protocol:
- Packet format:
  - `[TYPE]`: 1 byte
  - If `[TYPE]` == `StateReport`
    - `[SESSION_ID]`: 4 bytes : Last session id received from dongle.
    - Hid report containing full state.
  - If `[TYPE]` == `AcknowledgeState`
    - `[STATE_COUNTER]`: 4 bytes: Counter associated with the acknowledged state.
      - If multiple packets were recenttly received, this should be the newest one.
  - If `[TYPE]` == `NewSession`
    - `[SESSION_ID]`: 4 bytes: Random id only generated once when the dongle first boots up.
  - Both the keyboard and dongle still need to maintain state, but that's mainly to ensure that counters are never re-used. 
  - All packets are padded to 28 bytes with zero bytes.
    - So the exact cipher text size is 32 bytes after MIC is added.

- On key up/down,
  - We will send a packet including the full state to the dongle with a 'StateReport' packet.
  - The dongle should send back an Ack packet with the counter of the latest state report received
    - The keyboard is guranteed not to send another packet for at last 0.5ms
    - If the keyboard doesn't receive an Ack packet soon, it will re-send the packet
  - The dongle records the latest received counter value 
    - But If the dongle turns off, it may miss some presses.
    - it's possible that a user enters their password and later an attacker replays that onto the dongle.
- So when the dongle starts up, if it observes a keyboard sending stuff, it will:
  - Instead of doing an Ack, will request the keyboard use a new random session_id.
  - Upon receiving this request the keyboard will resend the next packet using the random seed. 
  - The keyboard will always only prefer the random seed with the 
- If the dongle hasn't received a packet in 5 minutes it will generate a new SESSION_ID to be used in 
- The dongle should reject any packets received with out of order packet counters.

- Actually, we should'd need to do a 'NewSesSion'. Instead we mainly need a ping/pong so that one side can verify what the largest counter of the other side is. 

Known issues:
- If we store the last counter value in an insecure eeprom, it could be re-wound to a previous value.
- Both devices know the shared key so could impersonate each other.
- If we are already communicating very well, we don't want a bad actor to start issueing many un-encrypted NewSession requests as these can take a long time to authenticate.

- We could bypass a need for persistent state if we generate a new session key every time.
  - Main issue is that we would need key exchange to not be encrypted but it must be authenticated.

The best strategy is to rely only only a single public/private key certificate to establish per-session keys which are only ever stored in RAM.
- This also requires that writing to flash in the bootloader is protected by a key.
- We don't need to store a full certificate as we should know the certificate for each address from a database.
  - Mainly just need to store 

## V2 Protocol

- Each device only contains a per-device (private key, public key) in non-volatile storage
  - This almost never changes so can be stored in NRF52 flash (with APP protect enabled).
- All devices in the network are aware of the 'public key' of every other device they intend on connecting to.
- If a device doesn't have a session_key, it will create a random secret and send an EC point to the other device.
  - The EC point is signed with the private key so that it is easy for the other device to 
- It is still useful for the EEPROM to contain some volatile data like the network state.
  - This data is encrpyted with AES-CCM using a random IV and the secret per-device key (or a key derived from that).
- When we establish a new session_key, the packet counter resets to 0
- Differentiating between when a new and old key is in use.
  - On the recipient side, decrpytion will fail so they can just request a new key.
  - This way we don't need 
- I need to agree on a secret of length at least 21 bytes for AES key + IV
- Using Ed255219 to sign it

- Agreeing on a key:
  - One side sends a NewSession packet with signed ECDHE value
  - Other side sends back its own public value in a BeginSession with the other ECDHE value and an echo of the other device's value.
    - Then device 2 can immediately start sending encrypted values.
  - Device 1 may not get the second value. If this occurs it can re-send NewSesion with the same value
    - Device 2 can re-broadcast the same ECDHE value so long as it hasn't yet received properly encrypted values from Device 1.
- But, I don't want to rely on decryption failures?
  - I malicious user may start generating bad packets that can't be decrypted.
- Another issue is replaying of the NewSession requests.
  - Difficult to tell in a stateless way if they are old.
  - So after receiving a valid BeginSession, the recipient should also send back an AcceptSession packet which is encrypted.
  - Issue is that an AcceptSession packet may also be lost so need a stronger mechanism for switching ciphers
- We send a 

So algorithms I need are:
- Ed255219 for Signatures
- X255219 for diffie helman
- Alternative is to just use static keys for now.
- Major concern is DOS with NewSession packets.


- In the keyboard case,
  - Keyboard sends NewSession
  - Dongle responds with StartSession
  - Keyboard starts sending periodic encrypted StateReports
  - Dongle may reboot and no longer know how to communicate.


- NewSession packets are replay-able
  - But we could enforce monotonic packet counters for NewSession so that new keys 

- We won't protect against physical access but we will protect against remote replay attacks
  - Requires the dongle have an EEPROM to use for storing the last received packet.
  - At work I won't use this mode as it doesn't protect against physical access attacks (rather E2E encrypt that.)
- So for now ignore all the complexity.
  - Keyboard will rely on the simple 'good enough' protocol

- If we don't care about physical access protections, the simplest protocol would simply 


First:
- Sign the local diffi-helman secret and send it to the other device.
- Other device responds with 

This is a test of the new keyboard. The main issue is that the space bar is very void.  

Drone use-case
- Central master required to coordinate all operations
- If a dorne doesn't receive some packets, we don't want it to be replayed later.


Serial Abstraction:
- The first 4 bytes of the payload will be a U32 offset of the data.
- The data in each packet will be of the form:
  - 4KB serial send buffer cyclic.
  - If we enqueue too many things to send, we will stop trying to send old data.
- Similarly we will have a receive buffer.
  - It will 



## Old


Bluetooth LE nonce (according to Bluetooth Core Spec) is:
- 13 bytes
- Packet length is 2 bytes
- Format is:
  - First 4 octets are Payload bounder
  - Byte 5 is 6 packet counter bytes + 1 direction bit.
  - Final 8 bytes are the IV


Zigbee Nonce is:
- 8 bytes of source address
- 4 bytes of frame counter
- 1 byte of security control


TODO: Should enable ICACHE of flash

- 1024 kB of flash. Each page is 4 kB
  - 10,000 erase cycles
  - Partial 


Flashing a CC2531 sniffer:
- https://www.zigbee2mqtt.io/guide/adapters/flashing/alternative_flashing_methods.html


NOTE: connecting VDD and VDDH enables Normal voltage mode which bypasses REG0


Nice instructions for using CC2531 as Zigbee sniffer:
- https://www.zigbee2mqtt.io/advanced/zigbee/04_sniff_zigbee_traffic.html#with-cc2531


https://github.com/openocd-org/openocd/blob/master/tcl/board/nordic_nrf52_dk.cfg

