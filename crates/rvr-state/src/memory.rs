//! Guarded memory allocation with mmap.
//!
//! Provides a memory region with guard pages on each side to catch
//! buffer overflows/underflows at the OS level.

use nix::sys::mman::{mmap_anonymous, mprotect, munmap, MapFlags, ProtFlags};
use std::ffi::c_void;
use std::num::NonZeroUsize;
use std::ptr::NonNull;
use thiserror::Error;

/// Guard page size (16KB, must be >= page size and cover max load/store offset).
pub const GUARD_SIZE: usize = 1 << 14;

/// Default memory size (4GB).
pub const DEFAULT_MEMORY_SIZE: usize = 1 << 32;

/// Memory allocation error.
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("mmap failed: {0}")]
    MmapFailed(#[from] nix::Error),

    #[error("invalid memory size: {0}")]
    InvalidSize(usize),
}

/// Memory region with guard pages.
///
/// Allocates `[GUARD][MEMORY][GUARD]` with the guard pages protected as PROT_NONE.
/// Any access to guard pages will cause a segfault, catching buffer overflows.
pub struct GuardedMemory {
    /// Pointer to the start of the entire region (including first guard).
    region: NonNull<c_void>,
    /// Total size including both guard pages.
    total_size: usize,
    /// Size of the usable memory region.
    memory_size: usize,
}

impl GuardedMemory {
    /// Allocate a new guarded memory region.
    ///
    /// # Arguments
    ///
    /// * `memory_size` - Size of the usable memory region in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if mmap fails.
    pub fn new(memory_size: usize) -> Result<Self, MemoryError> {
        if memory_size == 0 {
            return Err(MemoryError::InvalidSize(memory_size));
        }

        let total_size = memory_size + 2 * GUARD_SIZE;

        // Allocate entire region as PROT_NONE
        let region = unsafe {
            mmap_anonymous(
                None,
                NonZeroUsize::new(total_size).unwrap(),
                ProtFlags::PROT_NONE,
                MapFlags::MAP_PRIVATE | MapFlags::MAP_NORESERVE,
            )?
        };

        // Make middle portion readable/writable
        let memory_start = unsafe {
            NonNull::new_unchecked((region.as_ptr() as *mut u8).add(GUARD_SIZE) as *mut c_void)
        };
        unsafe {
            mprotect(
                memory_start,
                memory_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
            )?;
        }

        Ok(Self {
            region,
            total_size,
            memory_size,
        })
    }

    /// Create with default memory size (4GB).
    pub fn with_default_size() -> Result<Self, MemoryError> {
        Self::new(DEFAULT_MEMORY_SIZE)
    }

    /// Returns pointer to usable memory (after first guard page).
    pub fn as_ptr(&self) -> *mut u8 {
        unsafe { (self.region.as_ptr() as *mut u8).add(GUARD_SIZE) }
    }

    /// Returns the size of the usable memory region.
    pub fn size(&self) -> usize {
        self.memory_size
    }

    /// Zero the entire memory region.
    pub fn clear(&mut self) {
        unsafe {
            std::ptr::write_bytes(self.as_ptr(), 0, self.memory_size);
        }
    }

    /// Copy data into memory at the given offset.
    ///
    /// # Safety
    ///
    /// Caller must ensure `offset + data.len() <= self.size()`.
    pub unsafe fn copy_from(&mut self, offset: usize, data: &[u8]) {
        debug_assert!(offset + data.len() <= self.memory_size);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.as_ptr().add(offset), data.len());
        }
    }

    /// Read a byte from memory.
    ///
    /// # Safety
    ///
    /// Caller must ensure `offset < self.size()`.
    pub unsafe fn read_u8(&self, offset: usize) -> u8 {
        debug_assert!(offset < self.memory_size);
        unsafe { *self.as_ptr().add(offset) }
    }

    /// Write a byte to memory.
    ///
    /// # Safety
    ///
    /// Caller must ensure `offset < self.size()`.
    pub unsafe fn write_u8(&mut self, offset: usize, value: u8) {
        debug_assert!(offset < self.memory_size);
        unsafe { *self.as_ptr().add(offset) = value };
    }
}

impl Drop for GuardedMemory {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(self.region, self.total_size);
        }
    }
}

// GuardedMemory is Send but not Sync (contains raw pointer)
unsafe impl Send for GuardedMemory {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guarded_memory_alloc() {
        let mem = GuardedMemory::new(4096).expect("allocation should succeed");
        assert_eq!(mem.size(), 4096);
        assert!(!mem.as_ptr().is_null());
    }

    #[test]
    fn test_guarded_memory_read_write() {
        let mut mem = GuardedMemory::new(4096).expect("allocation should succeed");

        unsafe {
            mem.write_u8(0, 0xAB);
            mem.write_u8(4095, 0xCD);
            assert_eq!(mem.read_u8(0), 0xAB);
            assert_eq!(mem.read_u8(4095), 0xCD);
        }
    }

    #[test]
    fn test_guarded_memory_copy() {
        let mut mem = GuardedMemory::new(4096).expect("allocation should succeed");
        let data = [1u8, 2, 3, 4, 5];

        unsafe {
            mem.copy_from(100, &data);
            assert_eq!(mem.read_u8(100), 1);
            assert_eq!(mem.read_u8(104), 5);
        }
    }

    #[test]
    fn test_guarded_memory_clear() {
        let mut mem = GuardedMemory::new(4096).expect("allocation should succeed");

        unsafe {
            mem.write_u8(0, 0xFF);
            mem.write_u8(100, 0xFF);
        }

        mem.clear();

        unsafe {
            assert_eq!(mem.read_u8(0), 0);
            assert_eq!(mem.read_u8(100), 0);
        }
    }

    #[test]
    fn test_guarded_memory_invalid_size() {
        let result = GuardedMemory::new(0);
        assert!(result.is_err());
    }
}
