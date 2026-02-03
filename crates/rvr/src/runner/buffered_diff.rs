//! BufferedDiffRunner - runner with buffered diff tracer for block-level comparison.
//!
//! Unlike DiffRunner which captures a single instruction's state, this runner
//! uses a ring buffer to capture multiple instructions' states for block-level
//! comparison.

use std::ffi::c_void;

use rvr_elf::ElfImage;
use rvr_ir::Xlen;
use rvr_state::{BufferedDiffTracer, DiffEntry, GuardedMemory, InstretSuspender, RvState};

use super::traits::{BufferedDiffEntry, RunnerImpl};

/// Default buffer capacity for buffered diff tracer.
const DEFAULT_BUFFER_CAPACITY: usize = 4096;

/// Typed runner with buffered diff tracer for block-level differential testing.
///
/// Uses `BufferedDiffTracer` to capture multiple instructions' states in a ring
/// buffer. After block execution, Rust can iterate over the captured entries
/// and compare them against another executor's step-by-step execution.
pub struct BufferedDiffRunner<X: Xlen, const NUM_REGS: usize> {
    state: RvState<X, BufferedDiffTracer<X>, InstretSuspender, NUM_REGS>,
    memory: GuardedMemory,
    elf_image: ElfImage<X>,
    /// Ring buffer for captured entries (owned by Rust, pointer passed to C).
    buffer: Vec<DiffEntry<X>>,
}

impl<X: Xlen, const NUM_REGS: usize> BufferedDiffRunner<X, NUM_REGS> {
    pub fn new(elf_image: ElfImage<X>, memory: GuardedMemory) -> Self {
        Self::with_capacity(elf_image, memory, DEFAULT_BUFFER_CAPACITY)
    }

    pub fn with_capacity(elf_image: ElfImage<X>, memory: GuardedMemory, capacity: usize) -> Self {
        let mut state: RvState<X, BufferedDiffTracer<X>, InstretSuspender, NUM_REGS> =
            RvState::new();
        state.set_memory(memory.as_ptr());
        let brk = elf_image.get_initial_program_break();
        state.brk = brk;
        state.start_brk = brk;
        // Initialize suspender to not suspend (max u64)
        state.suspender.disable();

        // Allocate buffer and set up tracer
        let mut buffer = vec![DiffEntry::default(); capacity];
        state.tracer.setup(buffer.as_mut_ptr(), capacity as u32);

        Self {
            state,
            memory,
            elf_image,
            buffer,
        }
    }

    /// Re-setup the tracer buffer pointer after reset.
    /// Must be called after state reset to reconnect buffer.
    fn reconnect_buffer(&mut self) {
        let capacity = self.buffer.len();
        self.state
            .tracer
            .setup(self.buffer.as_mut_ptr(), capacity as u32);
    }
}

impl<X: Xlen, const NUM_REGS: usize> RunnerImpl for BufferedDiffRunner<X, NUM_REGS> {
    fn load_segments(&mut self) {
        self.memory.clear();
        for seg in &self.elf_image.memory_segments {
            let vaddr = X::to_u64(seg.virtual_start) as usize;
            unsafe { self.memory.copy_from(vaddr, &seg.data) };
        }
    }

    fn reset(&mut self) {
        self.state.reset();
        self.state.set_memory(self.memory.as_ptr());
        self.state.suspender.disable();
        // Reconnect buffer after reset
        self.reconnect_buffer();
    }

    fn as_void_ptr(&mut self) -> *mut c_void {
        self.state.as_void_ptr()
    }

    fn instret(&self) -> u64 {
        self.state.instret()
    }

    fn exit_code(&self) -> u8 {
        self.state.exit_code()
    }

    fn has_exited(&self) -> bool {
        self.state.has_exited()
    }

    fn entry_point(&self) -> u64 {
        X::to_u64(self.elf_image.entry_point)
    }

    fn lookup_symbol(&self, name: &str) -> Option<u64> {
        self.elf_image.lookup_symbol(name)
    }

    fn set_register(&mut self, reg: usize, value: u64) {
        self.state.set_reg(reg, X::from_u64(value));
    }

    fn get_register(&self, reg: usize) -> u64 {
        X::to_u64(self.state.get_reg(reg))
    }

    fn get_pc(&self) -> u64 {
        X::to_u64(self.state.pc())
    }

    fn set_pc(&mut self, pc: u64) {
        self.state.set_pc(X::from_u64(pc));
    }

    fn get_csr(&self, csr: u16) -> u64 {
        X::to_u64(self.state.csrs[csr as usize])
    }

    fn set_csr(&mut self, csr: u16, value: u64) {
        self.state.csrs[csr as usize] = X::from_u64(value);
    }

    fn read_memory(&self, addr: u64, buf: &mut [u8]) -> usize {
        let mem_size = self.memory.size();
        let addr = addr as usize;
        if addr >= mem_size {
            return 0;
        }
        let len = buf.len().min(mem_size - addr);
        let src = unsafe { std::slice::from_raw_parts(self.memory.as_ptr().add(addr), len) };
        buf[..len].copy_from_slice(src);
        len
    }

    fn write_memory(&mut self, addr: u64, data: &[u8]) -> usize {
        let mem_size = self.memory.size();
        let addr = addr as usize;
        if addr >= mem_size {
            return 0;
        }
        let len = data.len().min(mem_size - addr);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.memory.as_ptr().add(addr), len);
        }
        len
    }

    fn num_regs(&self) -> usize {
        NUM_REGS
    }

    fn xlen(&self) -> u8 {
        X::VALUE
    }

    fn memory_size(&self) -> usize {
        self.memory.size()
    }

    fn clear_exit(&mut self) {
        self.state.clear_exit();
    }

    fn supports_suspend(&self) -> bool {
        true
    }

    fn get_target_instret(&self) -> Option<u64> {
        Some(self.state.suspender.target_instret)
    }

    fn set_target_instret(&mut self, target: u64) -> bool {
        self.state.suspender.set_target(target);
        true
    }

    // Buffered diff tracer methods

    fn buffered_diff_count(&self) -> Option<usize> {
        Some(self.state.tracer.len())
    }

    fn buffered_diff_has_overflow(&self) -> Option<bool> {
        Some(self.state.tracer.has_overflow())
    }

    fn buffered_diff_dropped(&self) -> Option<u32> {
        Some(self.state.tracer.dropped_count())
    }

    fn buffered_diff_get(&self, index: usize) -> Option<BufferedDiffEntry> {
        let entry = self.state.tracer.get(index)?;
        Some((
            X::to_u64(entry.pc),
            entry.opcode,
            entry.get_rd(),
            entry.get_rd_value(),
            entry.get_mem_access(),
        ))
    }

    fn buffered_diff_reset(&mut self) {
        self.state.tracer.reset();
    }
}
