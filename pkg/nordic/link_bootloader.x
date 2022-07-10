/* See https://docs.rust-embedded.org/embedonomicon/memory-layout.html */

MEMORY
{
  FLASH : ORIGIN = 0, LENGTH = 28K
  FLASH_PARAMS : ORIGIN = 28K, LENGTH = 4K
  APP_FLASH : ORIGIN = 0x8000, LENGTH = 1M - 32K
  RAM : ORIGIN = 0x20000000, LENGTH = 256K
  REGOUT0: ORIGIN = 0x10001304, LENGTH = 4
  PSELRESET: ORIGIN = 0x10001200, LENGTH = 8
}

ENTRY(entry);

EXTERN(RESET_VECTOR);

PHDRS
{
    text PT_LOAD;
    data PT_LOAD;
    regout0 PT_LOAD;
    pselreset PT_LOAD;
}

SECTIONS
{
    .vector_table ORIGIN(FLASH) :
    {
        /* First entry: initial Stack Pointer value */
        LONG(ORIGIN(RAM) + LENGTH(RAM));

        /* Second entry: reset vector */
        KEEP(*(.vector_table.reset_vector));
    } > FLASH :text

    .text :
    {
        *(.text .text.*);
    } > FLASH :text

    .rodata :
    {
        *(.rodata .rodata.*);
    } > FLASH :text

    .pselreset :
    {
        /* NOTE: The pin number changed for different MCUs. This is only valid for NRF52840 */
        LONG(18)
        LONG(18)
    } > PSELRESET :pselreset

    .regout0 :
    {
        LONG(5) /* Set to 3.3V VDD */
    } > REGOUT0 :regout0

    .bss : ALIGN(4)
    {
        _sbss = .;
        *(.bss.*);
        _ebss = ALIGN(4);
    } > RAM :NONE

    .data : ALIGN(4)
    {
        _sdata = .;
        *(.data.*);
        _edata = ALIGN(4);
    } > RAM AT > FLASH :data

    _sidata = LOADADDR(.data);

    .heap : ALIGN(4)
    {
        _sheap = .;
    } > RAM :NONE

    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.* .ARM.extab.*);
    } :NONE
}