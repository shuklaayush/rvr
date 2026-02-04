//! Guarded memory allocation with mmap.
//!
//! Provides a memory region with guard pages on each side to catch
//! buffer overflows/underflows at the OS level.

use nix::sys::mman::{MapFlags, ProtFlags, mmap_anonymous, mprotect, munmap};
use std::ffi::c_void;
use std::num::NonZeroUsize;
use std::ptr::NonNull;
use thiserror::Error;

/// Guard page size (16KB, must be >= page size and cover max load/store offset).
pub const GUARD_SIZE: usize = 1 << 14;

/// Get flags for fixed-address mmap that fails if address is already mapped.
///
/// Uses `MAP_FIXED_NOREPLACE` on Linux (safer - returns EEXIST if address is taken).
/// Falls back to `MAP_FIXED` on macOS/BSD (will unmap existing mappings).
#[cfg(target_os = "linux")]
const fn map_fixed_flags() -> MapFlags {
    MapFlags::MAP_FIXED_NOREPLACE
}

#[cfg(not(target_os = "linux"))]
fn map_fixed_flags() -> MapFlags {
    MapFlags::MAP_FIXED
}

/// Default memory size (4GB).
pub const DEFAULT_MEMORY_SIZE: usize = 1 << 32;

/// Memory allocation error.
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("mmap failed: {0}")]
    MmapFailed(#[from] nix::Error),

    #[error("invalid memory size: {0}")]
    InvalidSize(usize),

    #[error("fixed address {0:#x} is not available (already mapped or reserved)")]
    FixedAddressUnavailable(u64),
}

/// Memory region with guard pages.
///
/// Allocates `[GUARD][MEMORY][GUARD]` with the guard pages protected as `PROT_NONE`.
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

        let total_size = memory_size
            .checked_add(2 * GUARD_SIZE)
            .ok_or(MemoryError::InvalidSize(memory_size))?;
        let total_size_nz =
            NonZeroUsize::new(total_size).ok_or(MemoryError::InvalidSize(memory_size))?;

        // Allocate entire region as PROT_NONE
        let region = unsafe {
            mmap_anonymous(
                None,
                total_size_nz,
                ProtFlags::PROT_NONE,
                MapFlags::MAP_PRIVATE | MapFlags::MAP_NORESERVE,
            )?
        };

        // Make middle portion readable/writable
        let memory_start = unsafe {
            NonNull::new_unchecked(
                region
                    .as_ptr()
                    .cast::<u8>()
                    .add(GUARD_SIZE)
                    .cast::<c_void>(),
            )
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
    ///
    /// # Errors
    ///
    /// Returns an error if memory allocation fails.
    pub fn with_default_size() -> Result<Self, MemoryError> {
        Self::new(DEFAULT_MEMORY_SIZE)
    }

    /// Allocate memory at a specific fixed address.
    ///
    /// Uses `MAP_FIXED_NOREPLACE` to ensure the address is available.
    /// The usable memory starts at `fixed_addr`, with a guard page before it.
    ///
    /// # Arguments
    ///
    /// * `fixed_addr` - The address where the usable memory should start.
    /// * `memory_size` - Size of the usable memory region in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if mmap fails or the address is already in use.
    pub fn new_at_fixed(fixed_addr: u64, memory_size: usize) -> Result<Self, MemoryError> {
        use nix::errno::Errno;
        use std::num::NonZeroUsize;

        if memory_size == 0 {
            return Err(MemoryError::InvalidSize(memory_size));
        }

        let total_size = memory_size
            .checked_add(2 * GUARD_SIZE)
            .ok_or(MemoryError::InvalidSize(memory_size))?;
        let total_size_nz =
            NonZeroUsize::new(total_size).ok_or(MemoryError::InvalidSize(memory_size))?;

        // Region starts at (fixed_addr - GUARD_SIZE) to place usable memory at fixed_addr
        let region_start = usize::try_from(fixed_addr.saturating_sub(GUARD_SIZE as u64))
            .map_err(|_| MemoryError::InvalidSize(memory_size))?;

        // Use MAP_FIXED_NOREPLACE to fail if address is already mapped
        // This is safer than MAP_FIXED which would silently unmap existing mappings
        let region = unsafe {
            mmap_anonymous(
                Some(NonZeroUsize::new(region_start).ok_or(MemoryError::InvalidSize(memory_size))?),
                total_size_nz,
                ProtFlags::PROT_NONE,
                MapFlags::MAP_PRIVATE | MapFlags::MAP_NORESERVE | map_fixed_flags(),
            )
            .map_err(|e| {
                if e == Errno::EEXIST {
                    MemoryError::FixedAddressUnavailable(fixed_addr)
                } else {
                    MemoryError::MmapFailed(e)
                }
            })?
        };

        // Make middle portion readable/writable
        let memory_start = unsafe {
            NonNull::new_unchecked(
                region
                    .as_ptr()
                    .cast::<u8>()
                    .add(GUARD_SIZE)
                    .cast::<c_void>(),
            )
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

    /// Returns pointer to usable memory (after first guard page).
    #[must_use]
    pub const fn as_ptr(&self) -> *mut u8 {
        unsafe { self.region.as_ptr().cast::<u8>().add(GUARD_SIZE) }
    }

    /// Returns the size of the usable memory region.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.memory_size
    }

    /// Zero the entire memory region.
    pub const fn clear(&mut self) {
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
    #[must_use]
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

/// Fixed-address memory region (without guard pages).
///
/// Used for allocating state at a specific address for the fixed-addresses feature.
pub struct FixedMemory {
    addr: NonNull<c_void>,
    size: usize,
}

impl FixedMemory {
    /// Allocate memory at a specific fixed address.
    ///
    /// # Errors
    ///
    /// Returns an error if mmap fails or the fixed address is unavailable.
    pub fn new(fixed_addr: u64, size: usize) -> Result<Self, MemoryError> {
        use nix::errno::Errno;
        use std::num::NonZeroUsize;

        if size == 0 {
            return Err(MemoryError::InvalidSize(size));
        }

        let addr = usize::try_from(fixed_addr).map_err(|_| MemoryError::InvalidSize(size))?;
        let addr_nz = NonZeroUsize::new(addr).ok_or(MemoryError::InvalidSize(size))?;
        let size_nz = NonZeroUsize::new(size).ok_or(MemoryError::InvalidSize(size))?;

        let region = unsafe {
            mmap_anonymous(
                Some(addr_nz),
                size_nz,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_PRIVATE | map_fixed_flags(),
            )
            .map_err(|e| {
                if e == Errno::EEXIST {
                    MemoryError::FixedAddressUnavailable(fixed_addr)
                } else {
                    MemoryError::MmapFailed(e)
                }
            })?
        };

        Ok(Self { addr: region, size })
    }

    /// Returns pointer to the memory region.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut u8 {
        self.addr.as_ptr().cast::<u8>()
    }

    /// Returns the size of the memory region.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }
}

impl Drop for FixedMemory {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(self.addr, self.size);
        }
    }
}

unsafe impl Send for FixedMemory {}

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
