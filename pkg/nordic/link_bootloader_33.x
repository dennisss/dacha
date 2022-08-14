
MEMORY
{
    FLASH : ORIGIN = 0x00000000, LENGTH = 28K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
    PSELRESET : ORIGIN = 0x10001200, LENGTH = 8
    NFCPINS : ORIGIN = 0x1000120c, LENGTH = 4
    REGOUT0 : ORIGIN = 0x10001304, LENGTH = 4
}

ENTRY(entry);

PHDRS
{
    text PT_LOAD;
    data PT_LOAD;
    pselreset PT_LOAD;
    nfcpins PT_LOAD;
    regout0 PT_LOAD;
}

SECTIONS
{
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
    
    .pselreset :
    {
        LONG(18)
        LONG(18)
    } > PSELRESET :pselreset

    .nfcpins :
    {
        LONG(0)
    } > NFCPINS :nfcpins

    .regout0 :
    {
        LONG(5)
    } > REGOUT0 :regout0


    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.* .ARM.extab.*);
    } :NONE
    
}    
    