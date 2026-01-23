//! Runtime support for RISC-V programs targeting rvr.
//!
//! This crate provides:
//! - Linker script setup (via build.rs)
//! - Optional entry point (`_start`)
//! - Optional panic handler
//!
//! # Usage
//!
//! Add to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! rvr-rt = "0.1"
//! ```
//!
//! Then build with:
//! ```bash
//! rvr build --target rv64i .
//! ```

#![no_std]

use core::arch::global_asm;

// Entry point - sets up stack and global pointer, then calls main
#[cfg(feature = "entry")]
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

    # Call main
    call main

    # If main returns, trap
    ebreak

.size _start, . - _start
"#
);

/// Panic handler that halts (infinite loop)
#[cfg(feature = "panic-halt")]
#[panic_handler]
fn panic_halt(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Panic handler that traps (ebreak)
#[cfg(feature = "panic-trap")]
#[panic_handler]
fn panic_trap(_info: &core::panic::PanicInfo) -> ! {
    unsafe {
        core::arch::asm!("ebreak", options(noreturn));
    }
}
