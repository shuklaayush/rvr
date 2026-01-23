//! Bump allocator for rvr guest programs.
//!
//! A simple bump allocator that never frees memory. This is suitable
//! for short-lived programs that allocate and then exit.
//!
//! # Usage
//!
//! ```ignore
//! use rvr_rt::BumpAlloc;
//!
//! // 16 MB heap
//! #[global_allocator]
//! static ALLOC: BumpAlloc<{ 16 * 1024 * 1024 }> = BumpAlloc::new();
//! ```

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;

/// A simple bump allocator with a fixed-size heap.
///
/// The heap size is specified as a const generic parameter `N` in bytes.
/// Memory is never freed - the allocator just bumps a pointer forward.
///
/// # Thread Safety
///
/// This allocator is NOT thread-safe. It's designed for single-threaded
/// bare-metal RISC-V programs. The `Sync` implementation is provided
/// because global allocators require it, but concurrent access will
/// cause undefined behavior.
pub struct BumpAlloc<const N: usize> {
    heap: UnsafeCell<[u8; N]>,
    offset: UnsafeCell<usize>,
}

impl<const N: usize> BumpAlloc<N> {
    /// Create a new bump allocator.
    ///
    /// The heap is zero-initialized.
    pub const fn new() -> Self {
        Self {
            heap: UnsafeCell::new([0; N]),
            offset: UnsafeCell::new(0),
        }
    }

    /// Returns the heap capacity in bytes.
    pub const fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> Default for BumpAlloc<N> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<const N: usize> GlobalAlloc for BumpAlloc<N> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        let offset_ptr = self.offset.get();
        let current_offset = *offset_ptr;

        // Align the offset
        let aligned_offset = (current_offset + align - 1) & !(align - 1);

        // Check if we have enough space
        if aligned_offset + size > N {
            return ptr::null_mut();
        }

        // Update offset
        *offset_ptr = aligned_offset + size;

        // Return pointer to allocated memory
        (self.heap.get() as *mut u8).add(aligned_offset)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't deallocate - memory is reclaimed when program exits
    }
}

// SAFETY: This is only safe for single-threaded use. The Sync bound is required
// for global allocators, but concurrent access will cause UB.
unsafe impl<const N: usize> Sync for BumpAlloc<N> {}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::boxed::Box;

    #[test]
    fn test_basic_allocation() {
        // Use Box to ensure heap allocation during tests
        let alloc = Box::new(BumpAlloc::<1024>::new());

        unsafe {
            let ptr1 = alloc.alloc(Layout::from_size_align(64, 8).unwrap());
            assert!(!ptr1.is_null());

            let ptr2 = alloc.alloc(Layout::from_size_align(64, 8).unwrap());
            assert!(!ptr2.is_null());
            assert_ne!(ptr1, ptr2);
        }
    }

    #[test]
    fn test_alignment() {
        let alloc = Box::new(BumpAlloc::<1024>::new());

        unsafe {
            // Allocate 1 byte to misalign
            let _ = alloc.alloc(Layout::from_size_align(1, 1).unwrap());

            // Next allocation with 16-byte alignment should be aligned
            let ptr = alloc.alloc(Layout::from_size_align(32, 16).unwrap());
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % 16, 0);
        }
    }

    #[test]
    fn test_out_of_memory() {
        let alloc = Box::new(BumpAlloc::<64>::new());

        unsafe {
            // This should succeed
            let ptr1 = alloc.alloc(Layout::from_size_align(32, 8).unwrap());
            assert!(!ptr1.is_null());

            // This should fail (not enough space)
            let ptr2 = alloc.alloc(Layout::from_size_align(64, 8).unwrap());
            assert!(ptr2.is_null());
        }
    }
}
