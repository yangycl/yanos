ENTRY(_start)

PHDRS
{
    image PT_LOAD FLAGS(5);
}

SECTIONS
{
    . = 0x1000;
    .text :
    {
        KEEP(*(.text.start))
        *(.text .text.*)
        *(.rodata .rodata.*)
        *(.got .got.*)
        *(.data.rel.ro .data.rel.ro.*)
        *(.data .data.*)
    } :image

    /DISCARD/ :
    {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.gnu*)
        *(.interp)
        *(.dynamic*)
        *(.dynsym*)
        *(.dynstr*)
        *(.bss .bss.* COMMON)
    }
}
