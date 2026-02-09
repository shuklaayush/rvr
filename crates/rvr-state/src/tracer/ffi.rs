//! FFI callback functions for C → Rust tracer calls.
//!
//! When using the FFI tracer, generated C code calls these extern functions.
//! The `Tracer*` argument contains an `inner` pointer to the actual Rust tracer.
//!
//! # Safety
//!
//! The caller (C code) must ensure:
//! - `tracer` is a valid pointer to a `FfiTracerPtr` struct
//! - `tracer->inner` is a valid pointer to a `Box<dyn Tracer>`
//! - The tracer outlives all calls to these functions

use std::ffi::c_void;

/// Tracer behavior trait.
///
/// Implement this trait to create a custom tracer that can be called from
/// generated C code via FFI.
///
/// All methods have default no-op implementations, so you only need to
/// implement the ones you care about.
pub trait Tracer: Send {
    /// Called at basic block entry.
    fn trace_block(&mut self, _pc: u64) {}

    /// Called before each instruction.
    fn trace_pc(&mut self, _pc: u64, _op: u16) {}

    /// Called when instruction opcode is decoded.
    fn trace_opcode(&mut self, _pc: u64, _op: u16, _opcode: u32) {}

    /// Called on register read.
    fn trace_reg_read(&mut self, _pc: u64, _op: u16, _reg: u8, _value: u64) {}

    /// Called on register write.
    fn trace_reg_write(&mut self, _pc: u64, _op: u16, _reg: u8, _value: u64) {}

    /// Called on byte memory read.
    fn trace_mem_read_byte(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u8) {}

    /// Called on halfword memory read.
    fn trace_mem_read_halfword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u16) {}

    /// Called on word memory read.
    fn trace_mem_read_word(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u32) {}

    /// Called on doubleword memory read.
    fn trace_mem_read_dword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u64) {}

    /// Called on byte memory write.
    fn trace_mem_write_byte(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u8) {}

    /// Called on halfword memory write.
    fn trace_mem_write_halfword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u16) {}

    /// Called on word memory write.
    fn trace_mem_write_word(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u32) {}

    /// Called on doubleword memory write.
    fn trace_mem_write_dword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u64) {}

    /// Called when branch is taken.
    fn trace_branch_taken(&mut self, _pc: u64, _op: u16, _target: u64) {}

    /// Called when branch is not taken.
    fn trace_branch_not_taken(&mut self, _pc: u64, _op: u16, _target: u64) {}

    /// Called on CSR read.
    fn trace_csr_read(&mut self, _pc: u64, _op: u16, _csr: u16, _value: u64) {}

    /// Called on CSR write.
    fn trace_csr_write(&mut self, _pc: u64, _op: u16, _csr: u16, _value: u64) {}

    /// Called at end of execution.
    fn finalize(&mut self) {}
}

/// FFI tracer pointer struct matching C's `Tracer` typedef.
///
/// This must match the layout in generated `rv_tracer.h`:
/// ```c
/// typedef struct Tracer {
///     void* inner;
/// } Tracer;
/// ```
#[repr(C)]
pub struct FfiTracerPtr {
    pub inner: *mut c_void,
}

impl FfiTracerPtr {
    /// Create from a boxed tracer.
    ///
    /// # Safety
    /// The returned pointer must be passed to `drop_tracer` when done.
    #[must_use]
    pub fn from_boxed(tracer: Box<dyn Tracer>) -> Self {
        Self {
            inner: Box::into_raw(tracer).cast::<c_void>(),
        }
    }

    /// Get mutable reference to the tracer.
    ///
    /// # Safety
    /// `inner` must be a valid pointer from `from_boxed`.
    unsafe fn as_tracer_mut(&mut self) -> &mut dyn Tracer {
        unsafe { &mut **self.inner.cast::<Box<dyn Tracer>>() }
    }
}

// =============================================================================
// FFI Exports - called by generated C code
// =============================================================================

/// Initialize tracer (called at start of execution).
#[unsafe(no_mangle)]
pub const unsafe extern "C" fn trace_init(_tracer: *mut FfiTracerPtr) {
    // No-op for now - tracer is already initialized
}

/// Finalize tracer (called at end of execution).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_fini(tracer: *mut FfiTracerPtr) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer).as_tracer_mut().finalize();
        }
    }
}

/// Trace block entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_block(tracer: *mut FfiTracerPtr, pc: u64) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer).as_tracer_mut().trace_block(pc);
        }
    }
}

/// Trace instruction PC.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_pc(tracer: *mut FfiTracerPtr, pc: u64, op: u16) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer).as_tracer_mut().trace_pc(pc, op);
        }
    }
}

/// Trace instruction opcode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_opcode(tracer: *mut FfiTracerPtr, pc: u64, op: u16, opcode: u32) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer).as_tracer_mut().trace_opcode(pc, op, opcode);
        }
    }
}

/// Trace register read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_reg_read(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    reg: u8,
    value: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer).as_tracer_mut().trace_reg_read(pc, op, reg, value);
        }
    }
}

/// Trace register write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_reg_write(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    reg: u8,
    value: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_reg_write(pc, op, reg, value);
        }
    }
}

/// Trace byte memory read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_read_byte(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u8,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_read_byte(pc, op, addr, value);
        }
    }
}

/// Trace halfword memory read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_read_halfword(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u16,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_read_halfword(pc, op, addr, value);
        }
    }
}

/// Trace word memory read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_read_word(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u32,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_read_word(pc, op, addr, value);
        }
    }
}

/// Trace doubleword memory read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_read_dword(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_read_dword(pc, op, addr, value);
        }
    }
}

/// Trace byte memory write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_write_byte(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u8,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_write_byte(pc, op, addr, value);
        }
    }
}

/// Trace halfword memory write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_write_halfword(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u16,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_write_halfword(pc, op, addr, value);
        }
    }
}

/// Trace word memory write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_write_word(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u32,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_write_word(pc, op, addr, value);
        }
    }
}

/// Trace doubleword memory write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_mem_write_dword(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    addr: u64,
    value: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_mem_write_dword(pc, op, addr, value);
        }
    }
}

/// Trace branch taken.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_branch_taken(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    target: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer).as_tracer_mut().trace_branch_taken(pc, op, target);
        }
    }
}

/// Trace branch not taken.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_branch_not_taken(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    target: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_branch_not_taken(pc, op, target);
        }
    }
}

/// Trace CSR read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_csr_read(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    csr: u16,
    value: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer).as_tracer_mut().trace_csr_read(pc, op, csr, value);
        }
    }
}

/// Trace CSR write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn trace_csr_write(
    tracer: *mut FfiTracerPtr,
    pc: u64,
    op: u16,
    csr: u16,
    value: u64,
) {
    unsafe {
        if !tracer.is_null() && !(*tracer).inner.is_null() {
            (*tracer)
                .as_tracer_mut()
                .trace_csr_write(pc, op, csr, value);
        }
    }
}

// =============================================================================
// Example tracer implementations
// =============================================================================

// TODO: separate file
/// No-op tracer - all methods do nothing.
#[derive(Default)]
pub struct NoopTracer;

impl Tracer for NoopTracer {}

/// Counting tracer - counts events.
#[derive(Default)]
pub struct CountingTracer {
    pub blocks: u64,
    pub instructions: u64,
    pub reg_reads: u64,
    pub reg_writes: u64,
    pub mem_reads: u64,
    pub mem_writes: u64,
    pub branches_taken: u64,
    pub branches_not_taken: u64,
}

impl Tracer for CountingTracer {
    fn trace_block(&mut self, _pc: u64) {
        self.blocks += 1;
    }

    fn trace_pc(&mut self, _pc: u64, _op: u16) {
        self.instructions += 1;
    }

    fn trace_reg_read(&mut self, _pc: u64, _op: u16, _reg: u8, _value: u64) {
        self.reg_reads += 1;
    }

    fn trace_reg_write(&mut self, _pc: u64, _op: u16, _reg: u8, _value: u64) {
        self.reg_writes += 1;
    }

    fn trace_mem_read_byte(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u8) {
        self.mem_reads += 1;
    }

    fn trace_mem_read_halfword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u16) {
        self.mem_reads += 1;
    }

    fn trace_mem_read_word(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u32) {
        self.mem_reads += 1;
    }

    fn trace_mem_read_dword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u64) {
        self.mem_reads += 1;
    }

    fn trace_mem_write_byte(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u8) {
        self.mem_writes += 1;
    }

    fn trace_mem_write_halfword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u16) {
        self.mem_writes += 1;
    }

    fn trace_mem_write_word(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u32) {
        self.mem_writes += 1;
    }

    fn trace_mem_write_dword(&mut self, _pc: u64, _op: u16, _addr: u64, _value: u64) {
        self.mem_writes += 1;
    }

    fn trace_branch_taken(&mut self, _pc: u64, _op: u16, _target: u64) {
        self.branches_taken += 1;
    }

    fn trace_branch_not_taken(&mut self, _pc: u64, _op: u16, _target: u64) {
        self.branches_not_taken += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counting_tracer() {
        let mut tracer = CountingTracer::default();
        tracer.trace_block(0x1000);
        tracer.trace_pc(0x1000, 0);
        tracer.trace_reg_write(0x1000, 0, 1, 42);

        assert_eq!(tracer.blocks, 1);
        assert_eq!(tracer.instructions, 1);
        assert_eq!(tracer.reg_writes, 1);
    }

    #[test]
    fn test_ffi_tracer_ptr_layout() {
        use std::mem::size_of;
        // Must be pointer-sized for FFI
        assert_eq!(size_of::<FfiTracerPtr>(), size_of::<*mut c_void>());
    }
}
