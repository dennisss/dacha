# Nordic

## Building

`cargo build --package nordic --target thumbv7em-none-eabihf --release`

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

- Follow Adafruit guide for adding Open OCD
  - https://learn.adafruit.com/programming-microcontrollers-using-openocd-on-raspberry-pi/overview
  - Pins 24 and 25 for SWD

Create board file:

```
source [find interface/raspberrypi-native.cfg]
bcm2835gpio_swd_nums 25 24
transport select swd
source [find target/nrf52.cfg]
```

^ May need to change to 'interface/raspberrypi2-native.cfg' depending on the pi.


```
cargo build --package nordic --target thumbv7em-none-eabihf --release
scp -i ~/.ssh/id_cluster target/thumbv7em-none-eabihf/release/nordic pi@10.1.0.67:~/binary
openocd -f nrf52_pi.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program /home/pi/binary verify" -c reset -c exit
```




## Design

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

Packet format (what is sent over the wire):
- `[PREAMBLE]`: 1 byte. Standard for NRF 2Mbit protocol
- `[TO_ADDRESS]`: 4 bytes. Who we are sending the packet to.
- `[LENGTH: 1 byte]`
- `[FROM_ADDRESS]`: 4 bytes. Who is sending this packet.
- `[COUNTER]`: 4 bytes. Monotonically incremented on every 
- `[CIPHERTEXT: Up to 245 bytes]`
    - Encrypted using AES-CCM
      - Length size is 2 bytes
      - MIC length is 4 bytes
    - Every unique (TO_ADDRESS, FROM_ADDRESS) pair has as pre-shared 16-byte AES key and 6-byte IV.
    - We form the 13 byte AES-CCM Nonce as:
      - `[PACKET_COUNTER]`: 4 bytes
      - `[FROM_ADDRESS]`: 4 bytes
      - `[IV]`: 5 bytes
- `[MIC: 4 bytes]`
- `[CRC: 2 bytes]`
  - Uses IEEE802154 standard of CRC-16 starting with the first byte after the length

Plaintext payload format:
- `[FLAGS]`: 1 byte
  - Bit 7: END: Whether or not this is the final packet in a frame.
  - Bit 6: ACK: Whether or not we want the recipient to 
    - When both ACK and END are set, the packet is a response to an ACK (the payload is a 4 bytes packet counter)
  - Bits 0-3: SEQUENCE_NUM: Starting at 0, the sequence number of the current packet in a frame.
    - NOTE: All packets in a single frame will have sequential SEQUENCE_NUMs and sequential COUNTER values.

Address constraints:
- 4-byte addresses will be randomly generated on the host machine with the following constraints:
  - New addresses are unique to old addresses
  - Address doesn't contain any 0x00 bytes.
  - No sequence of 8 bits in the address looks like a pre-amble (0x55 or 0xAA)

Bootstrapping
- By default, every device starts with no radio address and no keys present.
- A device must be connected via USB to a hub machine which will:
  - Randomly generate an address, key, IV.
  - Store the generated parameters in a local database.
  - Send them to the device over USB.


### Storage

For persistent storage of parameters, we use an EEPROM with a 'file system' abstraction where each file is identified by a unique 32-bit id. To ensure that each file has a unique number, a global enum is maintained in code.

In designing the binary format of the data stored on the EEPROM, we had to satisfy the following requirements:

1. Must be forwards/backwards compatible.
  - With respect to changes in the storage format and w.r.t the code that is reading/writing to the EEPROM.
2. Prefer page aligned writes as most EEPROMs have a minimum page size for writing.
3. Wear leveling (spread writes over the entire page space of the EEPROM to minimize wear).
4. Atomic re-writes: As writing to EEPROM takes a while and we may need to write critical security parameters like counters, we would like to support atomically overwriting a file with graceful rollback if the system restarts in the middle of the write.
5. Performance
  - We expect reads to be infreqent and only to well known file ids.
  - Writes may occur ~1 time per second.
6. Dynamic file size. Should be able to grow files if they end up becoming larger in the future.

Non-requirements:
- Partial file writes: We assume that files are always completely overwritten.
- String/path based file names or directories
- Concurrent file access: We assume that there is exactly one exclusive lock to a file in software.
- Dynamic file ids
  - This will be a non-performant anti-pattern as opening 

#### V1 Format

With this format, the user is required to specify the maximum size of each file. Currently we don't support increasing this maximum size.

On the EEPROM, data is stored as follows:

- Root Directory Table: (First page of the EEPROM)
  - `[VERSION]`: 1 byte : Set to 0x01
  - `[NUM_FILES]`: 1 byte: Number of files stored on the EEPROM
  - For each file:
    - `[ID]`: 4 byte (uint32 little endian): id of the file representing by the data in this block.
    - `[PAGE_LENGTH]`: 1 byte (uint16 little endian): Maximum number of pages spanned by one copy of this file.
  - `[CRC-16]`
  - `[PADDING]`: Zero padding up to the next page offset.

- File Blocks
  - The file data blocks immediately follow the page used by the root directory table.
  - Each file starts on a page aligned offset on the EEPROM immediately after the previous file as listed in the root directory table.
  - All files are double buffered meaning that are actually implemented as two contiguous files spanning 2x the LENGTH.
  - The foamt of each span of pages for one copy of a file is:
    - `[WRITE_COUNT]`: 4 byte (uint32 little endian): Number of times the `[ID]` has been written to the EEPROM.
    - `[LENGTH]`: 2 bytes: (uint16 little endian): Actual number of bytes stored in the `[DATA]` field
    - `[DATA]`: Var length data of the file
    - `[CRC-16]`
    - `[PADDING]`: Zero padding up to the next page offset.


Pros:
- Simple to implement compared to other solutions.

Cons:
- Inefficient use of space due to the requirement to allocate the upper bound of expected file size.
- Sub-par wear leveling 
- Limited number of supported files.
- Creating a new file is not an atomic operation.
- No support for deleting files.

#### Beta Format


On the EEPROM, data is stored as follows:

- First page of the EEPROM
  - `[VERSION]`: 1 byte : Set to 0x02.
  - `[PADDING]`: Filed with zeros to fill the entire page.
  - `[CRC-16]`: Computed over all previous bytes in the page
    - On initial boot, we will detect that the CRC is invalid and set all pages to zeros before then configuring this
      first page.

- Data blocks (start at the 2nd page of the EEPROM):
  - Each 'block' spans an exact number of full pages where the first block starts on the 2nd page of the EEPROM and the 'i+1' block starts on the page immediately after the 'i' block.
  - Each block has the following format:
    - `[ID]`: 4 byte (uint32 little endian): id of the file representing by the data in this block.
      - A reserved value of 0 is used to indicate that there is no file (and the previous block contained the last one).
    - `[WRITE_COUNT]`: 4 byte (uint32 little endian): Number of times the `[ID]` has been written to the EEPROM.
      - On file updates, there will be multiple files with the same `[ID]` in the EEPROM. In this case, only the block with the largest `[WRITE_COUNT]` containing the data. All older blockers are implicitly deleted. 
    - `[LENGTH]`: 2 byte (uint16 little endian): number of bytes stored in the `[DATA]` field of this block.
    - `[DATA]`: Variable length user provided data.
    - `[CRC-16]`: Computed over `[ID]`, `[WRITE_COUNT]`, `[LENGTH]`, and `[DATA]`.
    - `[PADDING]`: Enough zero padding to go up to the next aligned page offset.

This design imposes the following constraints:
- 

Note that all data for a single file is currently only ever stored contiguously. Also deletions of 

When either writing a new file or overwriting an existing file, the 

When either writing a new file or overwriting an existing file, the data is appended as a new block at the end of the EEPROM (after the last valid block).
- Thus files with the same `[ID]` but at a higher offset in the EEPROM override earlier files with that id.
- I would prefer to delete at write time as I don't want to validate checksums 


Issues:
-> Partially written blocks will mess up future blocks.

How to defragmenet it?
- Could store 1 bit per page to know 

Generic parameters:
1. Page size
2. Number of pages. (Divide this by 8 to get the number of )

Memory we need:
1. 1 bit for each page of eeprom to tell if it is allocated.
2. Buffer of page_size to support reasonable reads (although we could get away with having smaller buffers)
3. (Optional) List of files mapped to their current offset.
  - Could be reduced to 2 bytes (4 bytes aligned) if we can statically store the list of files in known positions in RAM
  - for the purposes of initial deduplication, we do need to store each file in expanded form:
    - write count: 4
    - id: 4
    - offset: 2
    - length: 2
    - (12 bytes minimum?)
  - At least 6 bytes per file (this is mainly needed to speed up reading all files from a ).
  - Issue is that in order to know if a block is allocated, we do need to know 

- Alternatively we would explicitly mark things as deleted (could just zero out the id or write count)

NOTE: Can't count a new block as overriding an old one unless we can validate the CRC-16
=> Easier to mark the old block as "deleted"
  -> There may still be a case where we have two blocks with the same id.
  -> In this case, we must finish the deletion on re-load.
  -> 

Issue is still how fast will this scale.
- If we have many, then I only need to check the last two write_counts.
- If the newest one is invalid, we will mark the entry as bad on start up


for each page
- First byte is type:
  - Either Start block, continuation block, end, or full block

  - Start block is:
    4 byte id
    4 byte write_count
    2 byte offset to next block
    + 2 byte crc

  - Continuation block is
    2 byte offset to next block
    + 2 byte crc
  
  - End block
    1 byte length
    + 2 byte crc

  - Full block
    - 4 byte id
    - 4 byte write count
    - 1 byte length
    - 2 byte CRC
- Again challenges is that we can't trust a block unless it's CRC passes.
- 




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




Nice instructions for using CC2531 as Zigbee sniffer:
- https://www.zigbee2mqtt.io/advanced/zigbee/04_sniff_zigbee_traffic.html#with-cc2531


https://github.com/openocd-org/openocd/blob/master/tcl/board/nordic_nrf52_dk.cfg


Flashing using a Black Magic Probe

- Useful links:
  - https://github.com/blacksphere/blackmagic/wiki/Useful-GDB-commands
  - 

- Download GDB from:
  - https://developer.arm.com/tools-and-software/open-source-software/developer-tools/gnu-toolchain/gnu-rm/downloads
  - `sudo apt install libncurses5`

- `~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb`
- `target extended-remote /dev/ttyACM0`
- `monitor tpwr enable`: Power the device via 3.3V
- `monitor swdp_scan`
- `attach 1`
- `load target/thumbv7em-none-eabihf/release/nordic`


Updating black magic probe:
- `sudo apt install dfu-util`
- `sudo dfu-util -d 1d50:6018,:6017 -s 0x08002000:leave -D ~/Downloads/blackmagic-native.bin`


Programming the NRF dongle

- Info on the SWD interface:
  - https://infocenter.nordicsemi.com/index.jsp?topic=%2Fug_nrf52840_dongle%2FUG%2Fnrf52840_Dongle%2Fintro.html
  - Connector dimensions: https://www.tag-connect.com/wp-content/uploads/bsk-pdf-manager/TC2050-IDC_Datasheet_7.pdf

- There is a pogo adaptor:
  - https://www.thingiverse.com/thing:3384693
  - Pogo Pin P75-E2 Dia 1.3mm Length 16.5mm

- I have
  - P50-E2
    - http://digole.com/index.php?productID=521
  - Diameter: 0.68mm
  - Length 16.55mm
  - Full stroke: 2.65mm

My adapter:
- Switched from 0.7mm PogoPinDiam to 0.72
- Switched from 2mm to 6mm back extrusion
- From the back of the 2mm version, we want to have 11mm (7mm with the 6mm version)