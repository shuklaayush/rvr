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
    pub const fn setup(
        &mut self,
        data: *mut u8,
        data_capacity: u32,
        pc: *mut X::Reg,
        pc_capacity: u32,
    ) {
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
    pub const fn setup(&mut self, addr_bitmap: *mut u64) {
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
    pub const fn setup(&mut self, context: *mut std::ffi::c_void) {
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
/// Note: The FILE* is managed by C code (`trace_init` opens, `trace_fini` closes).
/// Rust just needs to provide the memory layout.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DebugTracer {
    /// File pointer (managed by C code, NULL until `trace_init` called).
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

/// Diff tracer state - captures single-instruction state for differential testing.
///
/// Uses bounded memory (~48 bytes for RV64). Only stores the most recent
/// instruction's effects. State is cleared on `trace_pc` and accumulated during
/// the instruction.
///
/// Matches C struct generated by `gen_tracer_diff`:
/// ```c
/// typedef struct Tracer {
///     uint64_t pc;
///     uint32_t opcode;
///     uint8_t rd;
///     uint64_t rd_value;
///     uint64_t mem_addr;
///     uint64_t mem_value;
///     uint8_t mem_width;
///     uint8_t is_write;
///     uint8_t has_rd;
///     uint8_t has_mem;
///     uint8_t valid;
/// } Tracer;
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DiffTracer<X: Xlen> {
    /// Program counter.
    pub pc: X::Reg,
    /// Raw instruction opcode.
    pub opcode: u32,
    /// Destination register (0 = none/x0).
    pub rd: u8,
    // Padding to align rd_value (3 bytes on 64-bit)
    _pad0: [u8; 3],
    /// Value written to rd.
    pub rd_value: X::Reg,
    /// Memory address accessed.
    pub mem_addr: X::Reg,
    /// Memory value read/written.
    pub mem_value: X::Reg,
    /// Memory access width (1/2/4/8 bytes).
    pub mem_width: u8,
    /// 1 = store, 0 = load.
    pub is_write: u8,
    /// Non-zero if register was written.
    pub has_rd: u8,
    /// Non-zero if memory was accessed.
    pub has_mem: u8,
    /// Non-zero if instruction was traced.
    pub valid: u8,
    // Padding to align struct (3 bytes on 64-bit)
    _pad1: [u8; 3],
}

impl<X: Xlen> Default for DiffTracer<X> {
    fn default() -> Self {
        Self {
            pc: X::from_u64(0),
            opcode: 0,
            rd: 0,
            _pad0: [0; 3],
            rd_value: X::from_u64(0),
            mem_addr: X::from_u64(0),
            mem_value: X::from_u64(0),
            mem_width: 0,
            is_write: 0,
            has_rd: 0,
            has_mem: 0,
            valid: 0,
            _pad1: [0; 3],
        }
    }
}

impl<X: Xlen> TracerState for DiffTracer<X> {
    const KIND: u32 = 7;
}

impl<X: Xlen> DiffTracer<X> {
    /// Reset the tracer state (called by `trace_pc` in C).
    pub const fn reset(&mut self) {
        self.valid = 0;
        self.has_rd = 0;
        self.has_mem = 0;
    }

    /// Check if the tracer captured valid instruction state.
    pub const fn is_valid(&self) -> bool {
        self.valid != 0
    }

    /// Get the destination register if one was written (None for x0 or no write).
    pub const fn get_rd(&self) -> Option<u8> {
        if self.has_rd != 0 && self.rd != 0 {
            Some(self.rd)
        } else {
            None
        }
    }

    /// Get the value written to rd if applicable.
    pub fn get_rd_value(&self) -> Option<u64> {
        if self.has_rd != 0 && self.rd != 0 {
            Some(X::to_u64(self.rd_value))
        } else {
            None
        }
    }

    /// Check if memory was accessed.
    pub const fn has_mem_access(&self) -> bool {
        self.has_mem != 0
    }

    /// Get memory access info if applicable.
    pub fn get_mem_access(&self) -> Option<(u64, u64, u8, bool)> {
        if self.has_mem != 0 {
            Some((
                X::to_u64(self.mem_addr),
                X::to_u64(self.mem_value),
                self.mem_width,
                self.is_write != 0,
            ))
        } else {
            None
        }
    }
}

/// Single instruction entry for buffered diff tracer.
///
/// Matches C struct:
/// ```c
/// typedef struct DiffEntry {
///     uint64_t pc;
///     uint32_t opcode;
///     uint8_t rd;
///     uint8_t has_rd;
///     uint8_t has_mem;
///     uint8_t is_write;
///     uint64_t rd_value;
///     uint64_t mem_addr;
///     uint64_t mem_value;
///     uint8_t mem_width;
///     uint8_t _pad[7];
/// } DiffEntry;
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DiffEntry<X: Xlen> {
    pub pc: X::Reg,
    pub opcode: u32,
    pub rd: u8,
    pub has_rd: u8,
    pub has_mem: u8,
    pub is_write: u8,
    pub rd_value: X::Reg,
    pub mem_addr: X::Reg,
    pub mem_value: X::Reg,
    pub mem_width: u8,
    _pad: [u8; 7],
}

impl<X: Xlen> Default for DiffEntry<X> {
    fn default() -> Self {
        Self {
            pc: X::from_u64(0),
            opcode: 0,
            rd: 0,
            has_rd: 0,
            has_mem: 0,
            is_write: 0,
            rd_value: X::from_u64(0),
            mem_addr: X::from_u64(0),
            mem_value: X::from_u64(0),
            mem_width: 0,
            _pad: [0; 7],
        }
    }
}

impl<X: Xlen> DiffEntry<X> {
    /// Get the destination register if one was written (None for x0 or no write).
    pub const fn get_rd(&self) -> Option<u8> {
        if self.has_rd != 0 && self.rd != 0 {
            Some(self.rd)
        } else {
            None
        }
    }

    /// Get the value written to rd if applicable.
    pub fn get_rd_value(&self) -> Option<u64> {
        if self.has_rd != 0 && self.rd != 0 {
            Some(X::to_u64(self.rd_value))
        } else {
            None
        }
    }

    /// Get memory access info if applicable.
    pub fn get_mem_access(&self) -> Option<(u64, u64, u8, bool)> {
        if self.has_mem != 0 {
            Some((
                X::to_u64(self.mem_addr),
                X::to_u64(self.mem_value),
                self.mem_width,
                self.is_write != 0,
            ))
        } else {
            None
        }
    }
}

/// Buffered diff tracer state - ring buffer of instruction entries.
///
/// Matches C struct:
/// ```c
/// typedef struct Tracer {
///     DiffEntry* buffer;
///     uint32_t capacity;
///     uint32_t head;
///     uint32_t count;
///     uint32_t dropped;
///     DiffEntry current;
///     uint8_t current_valid;
///     uint8_t _pad[7];
/// } Tracer;
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BufferedDiffTracer<X: Xlen> {
    pub buffer: *mut DiffEntry<X>,
    pub capacity: u32,
    pub head: u32,
    pub count: u32,
    pub dropped: u32,
    pub current: DiffEntry<X>,
    pub current_valid: u8,
    _pad: [u8; 7],
}

impl<X: Xlen> Default for BufferedDiffTracer<X> {
    fn default() -> Self {
        Self {
            buffer: std::ptr::null_mut(),
            capacity: 0,
            head: 0,
            count: 0,
            dropped: 0,
            current: DiffEntry::default(),
            current_valid: 0,
            _pad: [0; 7],
        }
    }
}

impl<X: Xlen> TracerState for BufferedDiffTracer<X> {
    const KIND: u32 = 8;
}

impl<X: Xlen> BufferedDiffTracer<X> {
    /// Setup with provided buffer.
    pub const fn setup(&mut self, buffer: *mut DiffEntry<X>, capacity: u32) {
        self.buffer = buffer;
        self.capacity = capacity;
        self.head = 0;
        self.count = 0;
        self.dropped = 0;
        self.current_valid = 0;
    }

    /// Number of entries captured.
    pub const fn len(&self) -> usize {
        self.count as usize
    }

    /// Check if any entries were dropped due to overflow.
    pub const fn has_overflow(&self) -> bool {
        self.dropped > 0
    }

    /// Number of entries dropped due to overflow.
    pub const fn dropped_count(&self) -> u32 {
        self.dropped
    }

    /// Check if buffer is empty.
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get entry at index (0 = oldest entry).
    ///
    /// Returns None if index is out of bounds or buffer is null.
    pub fn get(&self, index: usize) -> Option<&DiffEntry<X>> {
        if self.buffer.is_null() || index >= self.count as usize {
            return None;
        }
        // Ring buffer: oldest entry is at (head - count) % capacity
        let start = if self.count >= self.capacity {
            self.head as usize
        } else {
            0
        };
        let actual_idx = (start + index) % self.capacity as usize;
        // SAFETY: index is bounds-checked above, buffer is non-null
        unsafe { Some(&*self.buffer.add(actual_idx)) }
    }

    /// Iterate over all captured entries in order (oldest first).
    pub const fn iter(&self) -> BufferedDiffIterator<'_, X> {
        BufferedDiffIterator {
            tracer: self,
            index: 0,
        }
    }

    /// Reset the tracer state (keeps buffer allocation).
    pub const fn reset(&mut self) {
        self.head = 0;
        self.count = 0;
        self.dropped = 0;
        self.current_valid = 0;
    }
}

impl<'a, X: Xlen> IntoIterator for &'a BufferedDiffTracer<X> {
    type Item = &'a DiffEntry<X>;
    type IntoIter = BufferedDiffIterator<'a, X>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over buffered diff entries.
pub struct BufferedDiffIterator<'a, X: Xlen> {
    tracer: &'a BufferedDiffTracer<X>,
    index: usize,
}

impl<'a, X: Xlen> Iterator for BufferedDiffIterator<'a, X> {
    type Item = &'a DiffEntry<X>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.tracer.get(self.index)?;
        self.index += 1;
        Some(entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.tracer.len().saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl<X: Xlen> ExactSizeIterator for BufferedDiffIterator<'_, X> {}

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
    fn test_diff_layout() {
        use std::mem::offset_of;

        // pc: 8 + opcode: 4 + rd: 1 + pad: 3 + rd_value: 8 +
        // mem_addr: 8 + mem_value: 8 + mem_width: 1 + is_write: 1 +
        // has_rd: 1 + has_mem: 1 + valid: 1 + pad: 3 = 48 bytes
        assert_eq!(size_of::<DiffTracer<Rv64>>(), 48);

        // Verify field offsets match C struct layout
        assert_eq!(offset_of!(DiffTracer<Rv64>, pc), 0);
        assert_eq!(offset_of!(DiffTracer<Rv64>, opcode), 8);
        assert_eq!(offset_of!(DiffTracer<Rv64>, rd), 12);
        assert_eq!(offset_of!(DiffTracer<Rv64>, rd_value), 16);
        assert_eq!(offset_of!(DiffTracer<Rv64>, mem_addr), 24);
        assert_eq!(offset_of!(DiffTracer<Rv64>, mem_value), 32);
        assert_eq!(offset_of!(DiffTracer<Rv64>, mem_width), 40);
        assert_eq!(offset_of!(DiffTracer<Rv64>, is_write), 41);
        assert_eq!(offset_of!(DiffTracer<Rv64>, has_rd), 42);
        assert_eq!(offset_of!(DiffTracer<Rv64>, has_mem), 43);
        assert_eq!(offset_of!(DiffTracer<Rv64>, valid), 44);
    }

    #[test]
    fn test_tracer_kinds() {
        assert_eq!(<() as TracerState>::KIND, 0);
        assert_eq!(<PreflightTracer<Rv64> as TracerState>::KIND, 1);
        assert_eq!(<StatsTracer as TracerState>::KIND, 2);
        assert_eq!(<FfiTracer as TracerState>::KIND, 3);
        assert_eq!(<DebugTracer as TracerState>::KIND, 5);
        assert_eq!(<DiffTracer<Rv64> as TracerState>::KIND, 7);
        assert_eq!(<BufferedDiffTracer<Rv64> as TracerState>::KIND, 8);
    }

    #[test]
    fn test_diff_entry_layout() {
        use std::mem::offset_of;

        // pc: 8 + opcode: 4 + rd: 1 + has_rd: 1 + has_mem: 1 + is_write: 1 +
        // rd_value: 8 + mem_addr: 8 + mem_value: 8 + mem_width: 1 + pad: 7 = 48 bytes
        assert_eq!(size_of::<DiffEntry<Rv64>>(), 48);

        // Verify field offsets match C struct layout
        assert_eq!(offset_of!(DiffEntry<Rv64>, pc), 0);
        assert_eq!(offset_of!(DiffEntry<Rv64>, opcode), 8);
        assert_eq!(offset_of!(DiffEntry<Rv64>, rd), 12);
        assert_eq!(offset_of!(DiffEntry<Rv64>, has_rd), 13);
        assert_eq!(offset_of!(DiffEntry<Rv64>, has_mem), 14);
        assert_eq!(offset_of!(DiffEntry<Rv64>, is_write), 15);
        assert_eq!(offset_of!(DiffEntry<Rv64>, rd_value), 16);
        assert_eq!(offset_of!(DiffEntry<Rv64>, mem_addr), 24);
        assert_eq!(offset_of!(DiffEntry<Rv64>, mem_value), 32);
        assert_eq!(offset_of!(DiffEntry<Rv64>, mem_width), 40);
    }

    #[test]
    fn test_buffered_diff_tracer_layout() {
        use std::mem::offset_of;

        // buffer: 8 + capacity: 4 + head: 4 + count: 4 + dropped: 4 +
        // current: 48 + current_valid: 1 + pad: 7 = 80 bytes
        assert_eq!(size_of::<BufferedDiffTracer<Rv64>>(), 80);

        // Verify field offsets
        assert_eq!(offset_of!(BufferedDiffTracer<Rv64>, buffer), 0);
        assert_eq!(offset_of!(BufferedDiffTracer<Rv64>, capacity), 8);
        assert_eq!(offset_of!(BufferedDiffTracer<Rv64>, head), 12);
        assert_eq!(offset_of!(BufferedDiffTracer<Rv64>, count), 16);
        assert_eq!(offset_of!(BufferedDiffTracer<Rv64>, dropped), 20);
        assert_eq!(offset_of!(BufferedDiffTracer<Rv64>, current), 24);
        assert_eq!(offset_of!(BufferedDiffTracer<Rv64>, current_valid), 72);
    }

    #[test]
    fn test_buffered_diff_tracer_iteration() {
        // Create a buffer on the stack
        let mut buffer = [DiffEntry::<Rv64>::default(); 4];
        buffer[0].pc = 0x1000;
        buffer[0].opcode = 0x0000_0013; // NOP
        buffer[1].pc = 0x1004;
        buffer[1].opcode = 0x0000_0033; // ADD

        let tracer = BufferedDiffTracer::<Rv64> {
            buffer: buffer.as_mut_ptr(),
            capacity: 4,
            count: 2,
            head: 2, // Next write would go at index 2
            ..Default::default()
        };

        // Test get
        assert_eq!(tracer.get(0).unwrap().pc, 0x1000);
        assert_eq!(tracer.get(1).unwrap().pc, 0x1004);
        assert!(tracer.get(2).is_none());

        // Test iteration
        let pcs: Vec<u64> = tracer.iter().map(|e| e.pc).collect();
        assert_eq!(pcs, vec![0x1000, 0x1004]);
    }
}
