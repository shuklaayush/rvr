//! RunnerImpl trait for type-erased runner implementations.

use std::ffi::c_void;

/// Trait for type-erased runner implementations.
pub trait RunnerImpl {
    /// Load ELF segments into memory.
    fn load_segments(&mut self);

    /// Reset state to initial values.
    fn reset(&mut self);

    /// Get state as void pointer for FFI.
    fn as_void_ptr(&mut self) -> *mut c_void;

    /// Get instruction count.
    fn instret(&self) -> u64;

    /// Get exit code.
    fn exit_code(&self) -> u8;

    /// Get entry point from ELF.
    fn entry_point(&self) -> u64;

    /// Look up a symbol by name.
    fn lookup_symbol(&self, name: &str) -> Option<u64>;

    /// Set a register value.
    fn set_register(&mut self, reg: usize, value: u64);

    /// Get a register value.
    fn get_register(&self, reg: usize) -> u64;

    /// Get the program counter.
    fn get_pc(&self) -> u64;

    /// Set the program counter.
    fn set_pc(&mut self, pc: u64);

    /// Get a CSR value.
    fn get_csr(&self, csr: u16) -> u64;

    /// Set a CSR value.
    fn set_csr(&mut self, csr: u16, value: u64);

    /// Read memory at the given address into the buffer.
    /// Returns the number of bytes read.
    fn read_memory(&self, addr: u64, buf: &mut [u8]) -> usize;

    /// Write memory at the given address from the buffer.
    /// Returns the number of bytes written.
    fn write_memory(&mut self, addr: u64, data: &[u8]) -> usize;

    /// Get the number of general-purpose registers (16 for E, 32 for I).
    fn num_regs(&self) -> usize;

    /// Get the XLEN (32 or 64).
    fn xlen(&self) -> u8;

    /// Get the memory size.
    fn memory_size(&self) -> usize;

    /// Clear the exit flag to allow further execution.
    fn clear_exit(&mut self);

    /// Check if the runner supports instret suspension (for single-stepping).
    fn supports_suspend(&self) -> bool {
        false
    }

    /// Get the target instret for suspension.
    fn get_target_instret(&self) -> Option<u64> {
        None
    }

    /// Set the target instret for suspension.
    fn set_target_instret(&mut self, _target: u64) -> bool {
        false
    }
}
