//! Entry point for rvr guest programs.
//!
//! Provides `_start` that sets up the environment and calls `main`,
//! then exits via ecall with the return value as exit code.

use core::arch::global_asm;

// Entry point for RV32I/RV64I (standard ABI - syscall number in a7)
#[cfg(all(
    feature = "entry",
    any(target_arch = "riscv32", target_arch = "riscv64"),
    not(target_feature = "e")
))]
global_asm!(
    r#"
.section .text._start
.global _start
.type _start, @function
_start:
    # Set up global pointer (required for some code models)
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop

    # Set up stack pointer
    la sp, __stack_top

    # Zero the BSS section
    la t0, __bss_start
    la t1, __bss_end
1:
    bgeu t0, t1, 2f
    sw zero, 0(t0)
    addi t0, t0, 4
    j 1b
2:

    # Call main (returns exit code in a0)
    call main

    # Exit via ecall with syscall 93 (exit)
    # a0 already contains exit code from main
    li a7, 93
    ecall

    # Should never reach here, but trap if we do
    unimp

.size _start, . - _start
"#
);

// Entry point for RV32E/RV64E (embedded ABI - syscall number in t0)
#[cfg(all(
    feature = "entry",
    any(target_arch = "riscv32", target_arch = "riscv64"),
    target_feature = "e"
))]
global_asm!(
    r#"
.section .text._start
.global _start
.type _start, @function
_start:
    # Set up global pointer (required for some code models)
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop

    # Set up stack pointer
    la sp, __stack_top

    # Zero the BSS section
    la t0, __bss_start
    la t1, __bss_end
1:
    bgeu t0, t1, 2f
    sw zero, 0(t0)
    addi t0, t0, 4
    j 1b
2:

    # Call main (returns exit code in a0)
    call main

    # Exit via ecall with syscall 93 (exit)
    # a0 already contains exit code from main
    # RVE: syscall number in t0 (x5) since a7 doesn't exist
    li t0, 93
    ecall

    # Should never reach here, but trap if we do
    unimp

.size _start, . - _start
"#
);
