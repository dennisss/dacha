
/* See https://docs.rust-embedded.org/embedonomicon/memory-layout.html */

MEMORY
{
  BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
  FLASH : ORIGIN = 0x10000100, LENGTH = 8M - 0x100
  RAM : ORIGIN = 0x20000000, LENGTH = 264K
}

ENTRY(entry);

EXTERN(RESET_VECTOR);

SECTIONS
{
    .boot_loader ORIGIN(BOOT2) :
    {
        KEEP(*(.boot_loader*));
    } > BOOT2

    .vector_table ORIGIN(FLASH) :
    {
        /* First entry: initial Stack Pointer value */
        LONG(ORIGIN(RAM) + LENGTH(RAM));

        /* Second entry: reset vector */
        KEEP(*(.vector_table.reset_vector));
    } > FLASH

    .text :
    {
        *(.text .text.*);
    } > FLASH

    .rodata :
    {
        *(.rodata .rodata.*);
    } > FLASH

    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.*);
    }
}