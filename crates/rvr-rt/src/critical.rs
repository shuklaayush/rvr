//! Critical section implementation for rvr guest programs.
//!
//! Provides a single-threaded critical section that disables interrupts
//! via the mstatus CSR. This is compatible with the `critical-section` crate.
//!
//! # Usage
//!
//! Enable the `critical-section` feature and the `critical-section` crate
//! will automatically use this implementation.

#[cfg(all(
    feature = "critical-section",
    any(target_arch = "riscv32", target_arch = "riscv64")
))]
mod impl_riscv {
    use core::arch::asm;

    static mut CRITICAL_SECTION_NESTING: usize = 0;
    static mut MSTATUS_BACKUP: usize = 0;

    /// Acquire the critical section by disabling interrupts.
    ///
    /// This clears the MIE bit in mstatus to disable machine interrupts.
    /// Nested calls are tracked to ensure interrupts are only re-enabled
    /// when the outermost critical section is released.
    #[no_mangle]
    pub unsafe extern "C" fn _critical_section_1_0_acquire() -> u8 {
        let mstatus: usize;

        // Clear MIE bit (bit 3) and read old value
        asm!(
            "csrrci {}, mstatus, 0x8",
            out(reg) mstatus,
        );

        if CRITICAL_SECTION_NESTING == 0 {
            MSTATUS_BACKUP = mstatus;
        }
        CRITICAL_SECTION_NESTING += 1;

        0 // Token (unused but required by trait)
    }

    /// Release the critical section, potentially re-enabling interrupts.
    ///
    /// If this is the outermost critical section and interrupts were
    /// previously enabled, they will be re-enabled.
    #[no_mangle]
    pub unsafe extern "C" fn _critical_section_1_0_release(_token: u8) {
        CRITICAL_SECTION_NESTING -= 1;

        if CRITICAL_SECTION_NESTING == 0 {
            // Restore MIE bit if it was set before
            if MSTATUS_BACKUP & 0x8 != 0 {
                asm!("csrsi mstatus, 0x8");
            }
        }
    }
}

// Re-export for use
#[cfg(all(
    feature = "critical-section",
    any(target_arch = "riscv32", target_arch = "riscv64")
))]
#[allow(unused_imports)]
pub use impl_riscv::*;

// Dummy implementation for non-RISC-V targets (e.g., testing on host)
#[cfg(all(feature = "critical-section", not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod impl_dummy {
    /// Dummy acquire for non-RISC-V targets.
    #[no_mangle]
    pub unsafe extern "C" fn _critical_section_1_0_acquire() -> u8 {
        0
    }

    /// Dummy release for non-RISC-V targets.
    #[no_mangle]
    pub unsafe extern "C" fn _critical_section_1_0_release(_token: u8) {}
}

#[cfg(all(feature = "critical-section", not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
#[allow(unused_imports)]
pub use impl_dummy::*;
