#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use core::arch::asm;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
static mut CRITICAL_SECTION_NESTING: usize = 0;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
static mut MSTATUS_BACKUP: usize = 0;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
unsafe extern "C" fn _critical_section_1_0_acquire() -> u8 {
    let mstatus: usize;

    asm!(
        "csrrci {}, mstatus, 0x8",
        out(reg) mstatus,
    );

    if CRITICAL_SECTION_NESTING == 0 {
        MSTATUS_BACKUP = mstatus;
    }
    CRITICAL_SECTION_NESTING += 1;

    0
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
unsafe extern "C" fn _critical_section_1_0_release(_token: u8) {
    CRITICAL_SECTION_NESTING -= 1;

    if CRITICAL_SECTION_NESTING == 0 {
        if MSTATUS_BACKUP & 0x8 != 0 {
            asm!("csrsi mstatus, 0x8");
        }
    }
}
