/* Linker script for rvr riscv-arch-test target */

OUTPUT_ARCH("riscv")
ENTRY(rvtest_entry_point)

SECTIONS
{
    /* Code at 0x80000000 */
    . = 0x80000000;
    .text.init : { *(.text.init) }
    . = ALIGN(0x1000);
    .text : { *(.text) *(.text.*) }

    /* HTIF tohost/fromhost at fixed address */
    . = 0x80001000;
    .tohost ALIGN(0x1000) : { *(.tohost) }

    /* Signature region at fixed address for easy extraction */
    . = 0x80002000;
    .signature ALIGN(0x10) : {
        *(.signature)
    }

    /* Data sections follow */
    . = ALIGN(0x1000);
    .data : { *(.data) *(.data.*) *(.sdata) *(.sdata.*) }
    .rodata : { *(.rodata) *(.rodata.*) *(.srodata) *(.srodata.*) }
    .bss : { *(.bss) *(.bss.*) *(.sbss) *(.sbss.*) }

    _end = .;
}
