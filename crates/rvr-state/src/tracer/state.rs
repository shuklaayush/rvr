//! Tracer state types for FFI with generated C code.
//!
//! Each tracer kind has a corresponding `#[repr(C)]` struct that matches
//! the C `Tracer` typedef in the generated header.
//!
//! When no tracing is needed, use `()` which is a ZST and adds nothing
//! to the struct layout.

use rvr_ir::Xlen;

/// Marker trait for FFI-safe tracer state.
///
/// Types implementing this trait can be embedded in `RvState` and must:
/// - Have `#[repr(C)]` layout (or be ZST)
/// - Match the corresponding C `Tracer` struct exactly
pub trait TracerState: Default + Copy {
    /// Tracer kind ID for C API (matches `RV_TRACER_KIND`).
    const KIND: u32;
}

// No tracer - zero-sized type, adds nothing to struct
impl TracerState for () {
    const KIND: u32 = 0;
}

/// Preflight tracer state - records execution for replay/proofs.
///
/// Matches C struct:
/// ```c
/// typedef struct Tracer {
///     uint8_t* data;
///     uint32_t data_idx;
///     uint32_t data_capacity;
///     REG_TYPE* pc;
///     uint32_t pc_idx;
///     uint32_t pc_capacity;
/// } Tracer;
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PreflightTracer<X: Xlen> {
    pub data: *mut u8,
    pub data_idx: u32,
    pub data_capacity: u32,
    pub pc: *mut X::Reg,
    pub pc_idx: u32,
    pub pc_capacity: u32,
}

impl<X: Xlen> Default for PreflightTracer<X> {
    fn default() -> Self {
        Self {
            data: std::ptr::null_mut(),
            data_idx: 0,
            data_capacity: 0,
            pc: std::ptr::null_mut(),
            pc_idx: 0,
            pc_capacity: 0,
        }
    }
}

impl<X: Xlen> TracerState for PreflightTracer<X> {
    const KIND: u32 = 1;
}

impl<X: Xlen> PreflightTracer<X> {
    /// Setup with provided buffers.
    pub fn setup(&mut self, data: *mut u8, data_capacity: u32, pc: *mut X::Reg, pc_capacity: u32) {
        self.data = data;
        self.data_idx = 0;
        self.data_capacity = data_capacity;
        self.pc = pc;
        self.pc_idx = 0;
        self.pc_capacity = pc_capacity;
    }
}

/// Stats tracer state - counts memory accesses.
///
/// Matches C struct:
/// ```c
/// typedef struct Tracer {
///     uint64_t* addr_bitmap;
/// } Tracer;
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct StatsTracer {
    pub addr_bitmap: *mut u64,
}

impl TracerState for StatsTracer {
    const KIND: u32 = 2;
}

impl StatsTracer {
    /// Setup with address bitmap.
    pub fn setup(&mut self, addr_bitmap: *mut u64) {
        self.addr_bitmap = addr_bitmap;
    }
}

/// FFI tracer state - calls external Rust functions.
///
/// The actual tracing happens via extern functions, so the struct
/// just holds a context pointer.
///
/// Matches C struct:
/// ```c
/// typedef struct Tracer {
///     void* context;
/// } Tracer;
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FfiTracer {
    pub context: *mut std::ffi::c_void,
}

impl TracerState for FfiTracer {
    const KIND: u32 = 3;
}

impl FfiTracer {
    /// Setup with context pointer.
    pub fn setup(&mut self, context: *mut std::ffi::c_void) {
        self.context = context;
    }
}

/// Dynamic tracer state - runtime function pointers.
///
/// Allows selecting trace behavior at runtime without recompilation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DynamicTracer<X: Xlen> {
    pub context: *mut std::ffi::c_void,
    pub trace_pc: Option<unsafe extern "C" fn(*mut std::ffi::c_void, X::Reg, u16)>,
    pub trace_mem_read:
        Option<unsafe extern "C" fn(*mut std::ffi::c_void, X::Reg, u16, X::Reg, u64)>,
    pub trace_mem_write:
        Option<unsafe extern "C" fn(*mut std::ffi::c_void, X::Reg, u16, X::Reg, u64)>,
    pub trace_reg_write:
        Option<unsafe extern "C" fn(*mut std::ffi::c_void, X::Reg, u16, u8, X::Reg)>,
}

impl<X: Xlen> TracerState for DynamicTracer<X> {
    const KIND: u32 = 4;
}

/// Debug tracer state - writes PCs to file for debugging.
///
/// Matches C struct:
/// ```c
/// typedef struct Tracer {
///     FILE* fp;
///     uint64_t pcs;
/// } Tracer;
/// ```
///
/// Note: The FILE* is managed by C code (trace_init opens, trace_fini closes).
/// Rust just needs to provide the memory layout.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DebugTracer {
    /// File pointer (managed by C code, NULL until trace_init called).
    pub fp: *mut std::ffi::c_void,
    /// Count of traced PCs.
    pub pcs: u64,
}

impl Default for DebugTracer {
    fn default() -> Self {
        Self {
            fp: std::ptr::null_mut(),
            pcs: 0,
        }
    }
}

impl TracerState for DebugTracer {
    const KIND: u32 = 5;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;
    use std::mem::size_of;

    #[test]
    fn test_unit_is_zst() {
        assert_eq!(size_of::<()>(), 0);
    }

    #[test]
    fn test_preflight_layout() {
        // 8 (ptr) + 4 + 4 + 8 (ptr) + 4 + 4 = 32 bytes
        assert_eq!(size_of::<PreflightTracer<Rv64>>(), 32);
    }

    #[test]
    fn test_stats_layout() {
        // 8 (ptr) = 8 bytes
        assert_eq!(size_of::<StatsTracer>(), 8);
    }

    #[test]
    fn test_ffi_layout() {
        // 8 (ptr) = 8 bytes
        assert_eq!(size_of::<FfiTracer>(), 8);
    }

    #[test]
    fn test_debug_layout() {
        // 8 (ptr) + 8 (u64) = 16 bytes
        assert_eq!(size_of::<DebugTracer>(), 16);
    }

    #[test]
    fn test_tracer_kinds() {
        assert_eq!(<() as TracerState>::KIND, 0);
        assert_eq!(<PreflightTracer<Rv64> as TracerState>::KIND, 1);
        assert_eq!(<StatsTracer as TracerState>::KIND, 2);
        assert_eq!(<FfiTracer as TracerState>::KIND, 3);
        assert_eq!(<DebugTracer as TracerState>::KIND, 5);
    }
}
