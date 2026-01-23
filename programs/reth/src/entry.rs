#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use crate::run;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
unsafe extern "C" {
    static __stack_top: u8;
    #[link_name = "__global_pointer$"]
    static __global_pointer: u8;
    static mut __bss_start: u8;
    static mut __bss_end: u8;
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        // Set up global pointer (must disable relaxation)
        ".option push",
        ".option norelax",
        "la gp, __global_pointer$",
        ".option pop",
        // Set up stack pointer
        "la sp, __stack_top",
        // Jump to Rust entry
        "call _start_rust",
        // Should never return, but trap if it does
        "ebreak",
    );
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
pub extern "C" fn _start_rust() -> ! {
    // BSS section is zeroed in loader
    // unsafe {
    //     let bss_start = &raw mut __bss_start;
    //     let bss_end = &raw const __bss_end;
    //     let bss_len = bss_end as usize - bss_start as usize;
    //     core::ptr::write_bytes(bss_start, 0, bss_len);
    // }

    run();

    // Exit via ecall with syscall number 93 (exit)
    // RV32E/RV64E: syscall number in t0 (x5) since a7 doesn't exist
    // RV32I/RV64I: syscall number in a7 (x17) per standard ABI
    #[cfg(target_feature = "e")]
    unsafe {
        core::arch::asm!("li a0, 0", "li t0, 93", "ecall", options(noreturn));
    }
    #[cfg(not(target_feature = "e"))]
    unsafe {
        core::arch::asm!("li a0, 0", "li a7, 93", "ecall", options(noreturn));
    }
}
