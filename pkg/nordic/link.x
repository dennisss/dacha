/* See https://docs.rust-embedded.org/embedonomicon/memory-layout.html */

MEMORY
{
  FLASH : ORIGIN = 0, LENGTH = 1M
  RAM : ORIGIN = 0x20000000, LENGTH = 256K
}

ENTRY(entry);

EXTERN(RESET_VECTOR);

SECTIONS
{
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

    .bss (NOLOAD) : ALIGN(4)
    {
        . = ALIGN(4);
        __sbss = .;
        *(.bss .bss.*);
        *(COMMON); /* Uninitialized C statics */
        . = ALIGN(4); /* 4-byte align the end (VMA) of this section */
    } > RAM

    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.*);
    }
}