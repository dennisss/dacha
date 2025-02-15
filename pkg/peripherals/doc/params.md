# Parameter Storage

This doc describes the design of the `ParamStorage` interface which enables storing structured data in a micro controller's flash storage space. It is meant to be used for storing long term dynamic values needed by an MCU application. A quick summary of the design goals is below:

- Read/write from a key-value table where keys are 32-bit GUIDs and values are arbitrary byte strings.
    - We refer to each key-value row as a `Param`
- Wear leveling to avoid wearing out the maximum number of erase/write cycles of the storage.
- Atomic writes which are resilient to power lose.
  - Note: This requires that at least 2 flash pages are allocated for parameter storage.
- Low overhead (as EEPROM/flash space is normally fairly limited).
  - Current design has 4 bytes over overhead per param (not including the size of the id) and around 44 bytes of RAM
- Support for heap-less or small heap MCUs.
- Dynamic sized parameters

Non-goals/limitations:

- Full system support : This isn't designed to operate a full file system with a dynamic set of files and directories.
- Very large values : We can only store values that are up to around 4000 bytes in length (will be less if the pages are fairly small due to entry overheads).
- Error Correction : We assume that the memory is fairly reliable. We will only support error detection, but if there is sufficient memory corruption, new values of a 'file' may be silently forgotten and replaced with older values or no value.

## Flash Memory Model

We assume that we are operating with flash memory that roughly works as follows:

- Is split into separate 'pages' (each would be something like 4096 bytes in length).
- Writing can only be done by burning bits (e.g. switching the value from 1 to 0 but not the other way around).
- An erase of an entire page is required if we want to change a bit back to its original value (e.g. back to 1).
- We don't need to write an entire flash page all at once (we can partly write it and later finish writing the rest so long as we follow the above rules of only flipping bits to 0).
- The main metric for durability that we want to minimize is the number of times we have to erase a page over the lifetime of an application.

## Data Format

The storage system will sub divide the flash into pages which have a size that is a multiple of the physical page size of the flash media and whenever we start writing to a page for the first time, we erase all the corresponding physical pages. The data format of each page is as follows:

- `page_index` : 4 byte little endian counter.
    - Increments by 1 whenever we write a new page.
    - Valid values on pages with larger indexes overwrite values from smaller index pages.
- Padding if needed to align to a valid writeable offset (e.g. most flash models only support writing at 32-bit aligned offsets).
- Repeated list of `ParamEntry` structs. Each as the following format:
    - Header : 4 bytes (little endian u32). Bits are defined as follows:
        - `[0 (LSB), 16)` : Checksum computed over the `id` and `data` fields.
        - `[16, 28)` : `length` (12-bit)
        - `[28, 32)` (MSB) : `flags` (4-bit) with the following bit flags defined:
            - `parity` : the 4-bit flags are only valid if they have an odd number of 1 bits.
            - `checkpoint` : Whether or not this is a 'checkpoint' entry. Explained later.
            - `store_id` : Whether or not the `entry_id` is present.
            - `reserved` : For now always '1'
    - `entry_id` (optional) : 4 bytes
        - Present if the store_id flag is set.
        - If not present, this uses the same id as the previous entry.
    - `data` : `length` bytes.
        - This is the actual user data associated with the parameter.
    - Optional padding align to a writeable offset.
    - Overall an entry is valid if both the flags parity bit is correct and the checksum is correct. 

We assume that a fixed number of flash pages have been reserved for parameter storage. As such, we first start by writing entries to the first (0th index page) and go to the next page when we run out of space until eventually cycling back to the first one (each time continuing to increment the `page_index`).

Regular user parameters are stored with regular non-empty data entries with a `entry_id` for the first entry in a row in a page and normally have the `checkpoint` flag set to 0. The entries in a page are only valid if at least one entry in the page has `checkpoint=1` (explained later in the write process).

## Read Process

In order to find the value of parameter, during startup/init time, we scan every single page. Among all entries found for a given 'id', the 'latest' value has the highest `page_index`. If there are multiple entries on the same page, then the entry that is at a higher offset in the page is the latest.

After the initial startup scan, a 'pointer' to the latest value of each 'id' is stored in memory and updated on writes so more additional re-scans aren't required.

## Write Process

Starting with the 0th page, writes always happen by appending additional entries to the page with the highest `page_index`. Note that parameters never span more than 1 page and the old values of parameters are never directly deleted during a write. Once a page runs out of space, the next flash page is erased and used as `page_index + 1`. Note that the entire page MUST be erased before any writes happen to it.

After erasing a flash page located at index 'i' in memory, we must copy over the values of all parameters whose latest values are on the page at index '(i + 1) % num_pages' onto the page at index 'i'. This ensures that the next erase is safe. The final entry written during the erase/copy/write cycle will have `checkpoint=1` set. The presence of `checkpoint=1` somewhere in a page allows us to trust that the above procedure has completely fully and no entries on the '(i + 1) % num_pages' page are at risk of being deleted.
