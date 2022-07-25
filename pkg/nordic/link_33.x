
MEMORY
{
    FLASH : ORIGIN = 0x00008000, LENGTH = 480K
    RAM : ORIGIN = 0x20000000, LENGTH = 128K
}

ENTRY(entry);

PHDRS
{
    text PT_LOAD;
    data PT_LOAD;
}

SECTIONS
{
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
    