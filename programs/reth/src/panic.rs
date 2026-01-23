use core::fmt::Write;
use core::panic::PanicInfo;

const TOHOST: *mut u64 = 0x80001000 as *mut u64;
const FROMHOST: *mut u64 = 0x80001040 as *mut u64;

const SYSCALL_WRITE: u64 = 64;
const STDOUT_FD: u64 = 1;

/// HTIF syscall structure for host communication.
/// Standard HTIF uses 64-bit fields at offsets 0, 8, 16, 24.
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

/// Terminates execution with the given exit code.
fn htif_exit(code: u8) -> ! {
    unsafe {
        TOHOST.write_volatile(((code as u64) << 1) | 1);
    }
    loop {}
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
fn panic(info: &PanicInfo) -> ! {
    let mut writer = HtifWriter::new();
    let _ = write!(writer, "{info}");
    writer.flush();
    htif_exit(1)
}
