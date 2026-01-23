use core::alloc::Layout;

// Simple bump allocator
const HEAP_SIZE: usize = 100 * 1024 * 1024; // 100 MB
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
static mut HEAP_OFFSET: usize = 0;

pub struct BumpAllocator;

unsafe impl core::alloc::GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Align the offset
        let offset = (HEAP_OFFSET + align - 1) & !(align - 1);

        if offset + size > HEAP_SIZE {
            return core::ptr::null_mut();
        }

        HEAP_OFFSET = offset + size;
        core::ptr::addr_of_mut!(HEAP).cast::<u8>().add(offset)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't deallocate
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;
