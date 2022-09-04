Deleted
- server-search
- server-jobs
- server-renderer
- server-analytics

# Blob Storage

The goal with this design is to provide a method of storing structured data on EEPROM or flash storage media attached to an embedded micro controller.

- Read/write from a key-value table where keys are 32-bit GUIDs and values are arbitrary byte strings.
- Wear leveling to avoid wearing out the maximum number of erase/write cycles of the storage.
- Atomic writes which are resilient to power lose.
- Low overhead (as EEPROM/flash space is normally fairly limited).
- Support for heap-less or small heap MCUs.

Non-goals:
- Error correction : We assume that the memory is fairly reliable. We will only support error detection.

We will use the following example storage media for framing our design:




## Internal Design

### V1

TODO

### V2

Limitations:
- Files can only be stored in contiguous memory so fragmentation may limit the amount of usable space.


/*
Implement wear leveled storage of small values in flash.

- Each entry is 4 byte word containing
    - 2 byte entry size
    - 16-bit checksum (CRC-16) of data
-  N bytes of data.

- Important to erase at the right time.
-

For the alternative format:
- We can have a bitmap storing the occupied pages and another one containing the recently deleted ones.
    - The occupied bitmap will consider recently deleted pages as occupied until we run out of space.
    - This is to improve wear leveling
    - But this is difficult to persist across restarts.
- Alternatively maintain a last writen block pointer
    - Always find the next available slot when cycling.

Other things to store:
- In order to clear a block, we would also need to


Note that for now we are just interested in storing one type of thing.

Each entry could be of th form

Ideally I would use this to store both the Network keys and

A unique challenge of flash is that we need to delete a page in order to move onto the next one.

TODO: Startup time is currently very slow as we must loop through every blob for every page to finalize a checkpoint.

- Every file has a 32-bit ID

- First

Format is:
- First word of each flash page is a 32-bit page counter which increases by 1 whenever we need to rollover to using another page.
    - We will only consider this value if the page has >= 1 valid entry in it.
- Following the first word are entries until the end of the page.
- Each entry has the form:
    - 2 byte entry size
        - We only use the bottom 12 bits
        - If the top bit is set, then this entry has the same id as the previous entry in the same page.
    - 2 byte checksum (CRC-16)
    - 4 byte block id (only present if the bit if the entry size is not set)
- Once we overflow the size of single flash page, we erase the next page and start writing to it.
- In order to ensure some value always persist we require that if any block's latest value only exists on the block after the deleted one, then its value must be copied at the newly erased page (or if the previous page has space, that one can be used instead).

- Practicality adjustments:
    - Any unknown block ids are subject to deletion.

- Adaption for EEPROM
    - Would need to support spanning to multiple pages.
    - Fragmentation is an issue as we'd need to support multi-page writes.

- If we have a free block map we could


TODO: With an EEPROM because we write at the same time as erasing, then if power goes out during a write we may see stale entries in the unwritten part of the page.
- for this case we would need to use a whole page CRC rather than using a per-entry CRC.

NOTE: The header of each entry must always be 32-bit aligned to make it easier to write to flash in words.

NOTE: The size of a write is limited to the size of one page.


Option 1:


*/