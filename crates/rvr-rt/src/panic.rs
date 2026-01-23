//! Panic handlers for rvr guest programs.
//!
//! Multiple options available via feature flags (mutually exclusive):
//! - `panic-halt`: Infinite loop (safe, debugger-friendly)
//! - `panic-trap`: Illegal instruction (exit_code=1 via trap)
//! - `panic-abort`: Exit via ecall with code 1
//! - `panic-htif`: Write panic message via HTIF, then exit with code 1

// Ensure only one panic handler is selected
#[cfg(all(feature = "panic-halt", feature = "panic-trap"))]
compile_error!("Features `panic-halt` and `panic-trap` are mutually exclusive");

#[cfg(all(feature = "panic-halt", feature = "panic-abort"))]
compile_error!("Features `panic-halt` and `panic-abort` are mutually exclusive");

#[cfg(all(feature = "panic-halt", feature = "panic-htif"))]
compile_error!("Features `panic-halt` and `panic-htif` are mutually exclusive");

#[cfg(all(feature = "panic-trap", feature = "panic-abort"))]
compile_error!("Features `panic-trap` and `panic-abort` are mutually exclusive");

#[cfg(all(feature = "panic-trap", feature = "panic-htif"))]
compile_error!("Features `panic-trap` and `panic-htif` are mutually exclusive");

#[cfg(all(feature = "panic-abort", feature = "panic-htif"))]
compile_error!("Features `panic-abort` and `panic-htif` are mutually exclusive");

#[cfg(any(feature = "panic-halt", feature = "panic-trap", feature = "panic-abort"))]
use core::panic::PanicInfo;

/// Panic handler that halts (infinite loop).
///
/// This is the safest option - the program will hang but won't
/// corrupt any state. Useful for debugging with a debugger attached.
#[cfg(feature = "panic-halt")]
#[panic_handler]
fn panic_halt(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Panic handler that traps via illegal instruction.
///
/// This triggers a trap with exit_code=1, indicating an error.
/// The `unimp` instruction is guaranteed to be illegal on all RISC-V.
#[cfg(feature = "panic-trap")]
#[panic_handler]
fn panic_trap(_info: &PanicInfo) -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe {
        core::arch::asm!("unimp", options(noreturn));
    }

    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    loop {
        core::hint::spin_loop();
    }
}

/// Panic handler that exits via ecall with code 1.
///
/// This cleanly exits the program with exit_code=1 via the
/// standard exit syscall (93).
#[cfg(feature = "panic-abort")]
#[panic_handler]
fn panic_abort(_info: &PanicInfo) -> ! {
    #[cfg(all(
        any(target_arch = "riscv32", target_arch = "riscv64"),
        not(target_feature = "e")
    ))]
    unsafe {
        // Standard ABI: syscall number in a7
        core::arch::asm!(
            "li a0, 1",  // exit code 1
            "li a7, 93", // syscall 93 = exit
            "ecall",
            options(noreturn)
        );
    }

    #[cfg(all(
        any(target_arch = "riscv32", target_arch = "riscv64"),
        target_feature = "e"
    ))]
    unsafe {
        // RVE ABI: syscall number in t0
        core::arch::asm!(
            "li a0, 1",  // exit code 1
            "li t0, 93", // syscall 93 = exit
            "ecall",
            options(noreturn)
        );
    }

    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    loop {
        core::hint::spin_loop();
    }
}

// HTIF panic handler implementation
#[cfg(feature = "panic-htif")]
mod htif {
    use core::fmt::Write;
    use core::panic::PanicInfo;

    const TOHOST: *mut u64 = 0x80001000 as *mut u64;
    const FROMHOST: *mut u64 = 0x80001040 as *mut u64;

    const SYSCALL_WRITE: u64 = 64;
    const STDOUT_FD: u64 = 1;

    /// HTIF syscall structure for host communication.
    #[repr(C, align(8))]
    struct Syscall {
        num: u64,
        fd: u64,
        buf: u64,
        len: u64,
    }

    /// Writes a buffer to the host via HTIF.
    fn htif_write(buf: &[u8]) {
        if buf.is_empty() {
            return;
        }
        let syscall = Syscall {
            num: SYSCALL_WRITE,
            fd: STDOUT_FD,
            buf: buf.as_ptr() as u64,
            len: buf.len() as u64,
        };

        unsafe {
            TOHOST.write_volatile(&syscall as *const _ as u64);
            // Wait for HTIF acknowledgment
            while FROMHOST.read_volatile() == 0 {}
            FROMHOST.write_volatile(0);
        }
    }

    /// Terminates execution with the given exit code via HTIF.
    fn htif_exit(code: u8) -> ! {
        unsafe {
            TOHOST.write_volatile(((code as u64) << 1) | 1);
        }
        loop {
            core::hint::spin_loop();
        }
    }

    /// Buffered writer for HTIF output.
    struct HtifWriter {
        buf: [u8; 256],
        pos: usize,
    }

    impl HtifWriter {
        const fn new() -> Self {
            Self {
                buf: [0; 256],
                pos: 0,
            }
        }

        fn flush(&mut self) {
            if self.pos > 0 {
                htif_write(&self.buf[..self.pos]);
                self.pos = 0;
            }
        }
    }

    impl Write for HtifWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.pos >= self.buf.len() {
                    self.flush();
                }
                self.buf[self.pos] = b;
                self.pos += 1;
            }
            Ok(())
        }
    }

    #[panic_handler]
    fn panic_htif(info: &PanicInfo) -> ! {
        let mut writer = HtifWriter::new();
        let _ = write!(writer, "{info}");
        writer.flush();
        htif_exit(1)
    }
}
