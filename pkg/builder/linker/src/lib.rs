/*
Utility for creating a linker script for building binaries for C&ortex M processors.

We assume that the processor has two main regions of memory:
- FLASH
- RAM

NOTEs:
- The reason we define our own program header assignment as the default assignment trys to load the program headers
themselves into memory over the bootloader

TODOS:
- After linking verify that the program with stack actually fits into memory.
- Format the LENGTH values nicely (e.g. 256K or 1M)
- Gurantee that all mmory regions are properly sorted.
*/

extern crate common;

use common::errors::*;
use common::line_builder::LineBuilder;

pub const WORD_SIZE: u32 = 4;

pub struct MemoryRange {
    pub origin: u32,
    pub length: u32,
}

pub struct NonVolatileRegister {
    pub name: String,
    pub address: u32,
    pub words: Vec<u32>,
}

pub struct CortexMCPUConfig {
    /// Total region of all normal flash memory available for user programs.
    pub flash: MemoryRange,

    /// Total region of all normal RAM memory available for user programs.
    pub ram: MemoryRange,

    pub registers: Vec<NonVolatileRegister>,

    /// Number of bytes at the beginning of flash which are reserved for storing
    /// bootloader related data.
    pub bootloader_reserved_bytes: u32,

    /// Number of bytes starting at index 0 of flash which can be used for
    /// storing the bootloader program. The rest up to bootloader_reserved_bytes
    /// might be used by the bootloader for other purposes such as storing
    /// parameters.
    pub bootloader_usable_bytes: u32,

    /// Number of bytes to leave unused at the end of flash. This is reserved
    /// for dynamic application parameters.
    pub param_reserved_bytes: u32,

    /// If true, the program should be loaded from flash into RAM.
    /// Generally this is only needed if the program is able to re-program
    /// itself.
    pub execute_from_ram: bool,

    pub building_bootloader: bool,
}

pub fn get_chip_config(chip_name: &str, building_bootloader: bool) -> Result<CortexMCPUConfig> {
    const NRF_BOOTLOADER_RESERVED: u32 = 32 * 1024;
    // Final page is reserved for bootloader params.
    const NRF_BOOTLOADER_USABLE: u32 = 28 * 1024;

    // Reserve 4 pages of flash for runtime parameter storage.
    const NRF_END_RESERVED_BYTES: u32 = 4 * 4096;

    match chip_name {
        "nrf52840" => {
            let mut config = CortexMCPUConfig {
                flash: MemoryRange {
                    origin: 0,
                    length: 1 * 1024 * 1024, // 1MB
                },
                ram: MemoryRange {
                    origin: 0x20000000,
                    length: 256 * 1024, // 256KB
                },
                registers: vec![],

                bootloader_reserved_bytes: NRF_BOOTLOADER_RESERVED,
                bootloader_usable_bytes: NRF_BOOTLOADER_USABLE,
                execute_from_ram: building_bootloader,
                param_reserved_bytes: NRF_END_RESERVED_BYTES,
                building_bootloader,
            };

            if building_bootloader {
                // NOTE: This pin number can vary between chips.
                config.registers.push(NonVolatileRegister {
                    name: "PSELRESET".into(),
                    address: 0x10001200,
                    words: vec![18, 18],
                });

                config.registers.push(NonVolatileRegister {
                    name: "REGOUT0".into(),
                    address: 0x10001304,
                    words: vec![5], // Set to 3.3V VDD
                });
            }

            Ok(config)
        }
        "nrf52833" => {
            let mut config = CortexMCPUConfig {
                flash: MemoryRange {
                    origin: 0,
                    length: 512 * 1024, // 512KB
                },
                ram: MemoryRange {
                    origin: 0x20000000,
                    length: 128 * 1024, // 128KB
                },
                registers: vec![],

                bootloader_reserved_bytes: NRF_BOOTLOADER_RESERVED,
                bootloader_usable_bytes: NRF_BOOTLOADER_USABLE,
                execute_from_ram: building_bootloader,
                param_reserved_bytes: NRF_END_RESERVED_BYTES,
                building_bootloader,
            };

            if building_bootloader {
                // NOTE: This pin number can vary between chips.
                config.registers.push(NonVolatileRegister {
                    name: "PSELRESET".into(),
                    address: 0x10001200,
                    words: vec![18, 18],
                });

                // TODO: This needs to be board specific.
                config.registers.push(NonVolatileRegister {
                    name: "NFCPINS".into(),
                    address: 0x1000120C,
                    words: vec![0], // Disabled. Act as GPIO pins.
                });

                config.registers.push(NonVolatileRegister {
                    name: "REGOUT0".into(),
                    address: 0x10001304,
                    words: vec![5], // Set to 3.3V VDD
                });
            }

            Ok(config)
        }
        _ => {
            return Err(format_err!("Unsupported chip named: {}", chip_name));
        }
    }
}

fn format_byte_length(num: u32) -> String {
    let megabyte = 1024 * 1024;
    let kilobyte = 1024;

    if num % megabyte == 0 {
        format!("{}M", num / megabyte)
    } else if num % kilobyte == 0 {
        format!("{}K", num / kilobyte)
    } else {
        format!("{}", num)
    }
}

pub fn generate_linker_script(config: &CortexMCPUConfig) -> Result<String> {
    let mut memory_regions = LineBuilder::new();
    let mut phdrs = LineBuilder::new();
    let mut sections = LineBuilder::new();

    let (flash_origin, flash_length) = {
        if config.building_bootloader {
            (config.flash.origin, config.bootloader_usable_bytes)
        } else {
            (
                config.flash.origin + config.bootloader_reserved_bytes,
                config.flash.length
                    - config.bootloader_reserved_bytes
                    - config.param_reserved_bytes,
            )
        }
    };

    memory_regions.add(format!(
        "FLASH : ORIGIN = 0x{origin:08x}, LENGTH = {length}",
        origin = flash_origin,
        length = format_byte_length(flash_length)
    ));
    phdrs.add("text PT_LOAD;");

    memory_regions.add(format!(
        "RAM : ORIGIN = 0x{origin:08x}, LENGTH = {length}",
        origin = config.ram.origin,
        length = format_byte_length(config.ram.length)
    ));

    // NOTE: Even though this will be contiguous in the file with the 'text'
    // section, it isn't contiguous in virtual memory so must be a separate section.
    phdrs.add("data PT_LOAD;");

    if config.execute_from_ram {
        sections.add(
            "
    .vector_table ORIGIN(FLASH) :
    {
        /* Vector table for executing entry() */
        LONG(ORIGIN(RAM) + LENGTH(RAM));
        KEEP(*(.entry_vector_table));
        . = ALIGN(4);
    } > FLASH :text

    /* Only code necessary for entry() will stay in flash. */
    .text : ALIGN(4)
    {
        *(.entry);
        . = ALIGN(4);
    } > FLASH :text

    .data ORIGIN(RAM) : ALIGN(4)
    {
        _vector_table = .;

        _sdata = .;

        /* Vector table for executing main() */
        LONG(ORIGIN(RAM) + LENGTH(RAM));
        KEEP(*(.vector_table.reset_vector));

        *(.text .text.*);
        *(.data.*);
        *(.rodata .rodata.*);
        _edata = ALIGN(4);
    } > RAM AT > FLASH :data    
        ",
        )
    } else {
        sections.add(
            "
    .vector_table ORIGIN(FLASH) :
    {
        _vector_table = .;
        LONG(ORIGIN(RAM) + LENGTH(RAM));
        KEEP(*(.vector_table.reset_vector));
        . = ALIGN(4);
    } > FLASH :text

    .text : ALIGN(4)
    {
        *(.entry);
        *(.text .text.*);
        . = ALIGN(4);
    } > FLASH :text

    .rodata : ALIGN(4)
    {
        *(.rodata .rodata.*);
        . = ALIGN(4);
    } > FLASH :text

    .data ORIGIN(RAM) : ALIGN(4)
    {
        _sdata = .;
        *(.data.*);
        _edata = ALIGN(4);
    } > RAM AT > FLASH :data
        ",
        );
    }

    sections.add(
        "
    _sidata = LOADADDR(.data);

    .bss : ALIGN(4)
    {
        _sbss = .;
        *(.bss.*);
        _ebss = ALIGN(4);
    } > RAM :NONE

    .heap : ALIGN(4)
    {
        _sheap = .;
    } > RAM :NONE
    ",
    );

    for register in &config.registers {
        let region_name = register.name.to_ascii_uppercase();
        let section_name = register.name.to_ascii_lowercase();

        memory_regions.add(format!(
            "{region_name} : ORIGIN = 0x{origin:08x}, LENGTH = {length}",
            region_name = region_name,
            origin = register.address,
            length = format_byte_length((register.words.len() as u32) * WORD_SIZE)
        ));
        phdrs.add(format!("{} PT_LOAD;", section_name));

        sections.add(format!(
            "    .{section_name} :",
            section_name = section_name
        ));
        sections.add("    {");
        for word in &register.words {
            sections.add(format!("        LONG({})", word));
        }
        sections.add(format!(
            "    }} > {region_name} :{section_name}",
            region_name = region_name,
            section_name = section_name
        ));
        sections.nl();
    }

    sections.add(
        "
    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.* .ARM.extab.*);
    } :NONE
    ",
    );

    memory_regions.indent_with("    ");
    phdrs.indent_with("    ");

    let out = format!(
        "
MEMORY
{{
{memory_regions}}}

ENTRY(entry);

PHDRS
{{
{phdrs}}}

SECTIONS
{{{sections}}}    
    ",
        memory_regions = memory_regions.to_string(),
        phdrs = phdrs.to_string(),
        sections = sections.to_string()
    );

    Ok(out)
}
