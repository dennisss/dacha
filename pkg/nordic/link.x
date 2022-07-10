/* See https://docs.rust-embedded.org/embedonomicon/memory-layout.html */

MEMORY
{
  FLASH : ORIGIN = 0, LENGTH = 1M
  RAM : ORIGIN = 0x20000000, LENGTH = 256K
  REGOUT0: ORIGIN = 0x10001304, LENGTH = 4
}

ENTRY(entry);

EXTERN(RESET_VECTOR);

SECTIONS
{
    _flash_start = ORIGIN(FLASH);
    _flash_end = ORIGIN(FLASH) + LENGTH(FLASH);

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

    .note.gnu.build-id :
    {
        *(.note.gnu.build-id);
    } > FLASH

    .regout0 :
    {
        LONG(5) /* Set to 3.3V VDD */
    } > REGOUT0

    .bss : ALIGN(4)
    {
        _sbss = .;
        *(.bss.*);
        _ebss = ALIGN(4);
    } > RAM

    .data : ALIGN(4)
    {
        _sdata = .;
        *(.data.*);
        _edata = ALIGN(4);
    } > RAM AT > FLASH

    _sidata = LOADADDR(.data);

    .heap : ALIGN(4)
    {
        _sheap = .;
    } > RAM

    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.* .ARM.extab.*);
    }
}