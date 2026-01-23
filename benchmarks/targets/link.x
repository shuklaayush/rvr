MEMORY
{
  /* Use lower half of 32-bit address space so zero-extension = sign-extension */
  RAM : ORIGIN = 0x40000000, LENGTH = 256M
}

ENTRY(_start)

SECTIONS
{
  .text : {
    *(.text._start)
    *(.text .text.*)
  } > RAM

  .rodata : {
    *(.rodata .rodata.*)
  } > RAM

  .data : {
    *(.data .data.*)
    PROVIDE(__global_pointer$ = . + 0x800);
  } > RAM

  .bss : {
    __bss_start = .;
    *(.bss .bss.*)
    *(COMMON)
    __bss_end = .;
  } > RAM

  __stack_top = ORIGIN(RAM) + LENGTH(RAM);

  /DISCARD/ : {
    *(.eh_frame)
  }
}
