/* See https://docs.rust-embedded.org/embedonomicon/memory-layout.html */

MEMORY
{
  FLASH : ORIGIN = 0x8000, LENGTH = 1M - 32K
  RAM : ORIGIN = 0x20000000, LENGTH = 256K
}

ENTRY(entry);

EXTERN(RESET_VECTOR);

/* Define our own program header assignment as the default assignment trys to load the program headers themselves into memory over the bootloader */
PHDRS
{
    /* NOTE: Even though these are contiguous in flash/file addresses, they are not contiguous in virtual memory so can't be mixed. */
    text PT_LOAD;
    data PT_LOAD;
}

SECTIONS
{
    _flash_start = ORIGIN(FLASH);
    _flash_end = ORIGIN(FLASH) + LENGTH(FLASH);

    .vector_table ORIGIN(FLASH) :
    {
        _vector_table = .;

        /* First entry: initial Stack Pointer value */
        LONG(ORIGIN(RAM) + LENGTH(RAM));

        /* Second entry: reset vector */
        KEEP(*(.vector_table.reset_vector));
    } > FLASH :text

    .text : ALIGN(4)
    {
        *(.entry);
        *(.text .text.*);
    } > FLASH :text

    .rodata : ALIGN(4)
    {
        *(.rodata .rodata.*);
    } > FLASH :text

    .data : ALIGN(4)
    {
        _sdata = .;
        *(.data.*);
        _edata = ALIGN(4);
    } > RAM AT > FLASH :data
    
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

    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.* .ARM.extab.*);
    } :NONE
}