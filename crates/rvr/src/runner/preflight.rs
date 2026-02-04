//! `PreflightRunner` - runner with preflight tracer for coverage analysis.

use std::ffi::c_void;

use rvr_elf::ElfImage;
use rvr_ir::Xlen;
use rvr_state::{GuardedMemory, PreflightTracer, RvState};

use super::RunnerImpl;

pub const PREFLIGHT_DATA_BYTES: usize = 1 << 20;
pub const PREFLIGHT_PC_ENTRIES: usize = 1 << 24;

/// Typed runner with preflight tracer (needs buffer management).
pub struct PreflightRunner<X: Xlen, const NUM_REGS: usize> {
    state: RvState<X, PreflightTracer<X>, (), NUM_REGS>,
    memory: GuardedMemory,
    elf_image: ElfImage<X>,
    data_buffer: Vec<u8>,
    pc_buffer: Vec<X::Reg>,
}

impl<X: Xlen, const NUM_REGS: usize> PreflightRunner<X, NUM_REGS> {
    pub fn new(elf_image: ElfImage<X>, memory: GuardedMemory) -> Self {
        let mut state = RvState::new();
        state.set_memory(memory.as_ptr());
        let brk = elf_image.get_initial_program_break();
        state.brk = brk;
        state.start_brk = brk;
        Self {
            state,
            memory,
            elf_image,
            data_buffer: vec![0u8; PREFLIGHT_DATA_BYTES],
            pc_buffer: vec![X::from_u64(0); PREFLIGHT_PC_ENTRIES],
        }
    }
}

impl<X: Xlen, const NUM_REGS: usize> RunnerImpl for PreflightRunner<X, NUM_REGS> {
    fn load_segments(&mut self) {
        self.memory.clear();
        for seg in &self.elf_image.memory_segments {
            let vaddr = usize::try_from(X::to_u64(seg.virtual_start))
                .expect("segment address does not fit in host usize");
            unsafe { self.memory.copy_from(vaddr, &seg.data) };
        }
    }

    fn reset(&mut self) {
        self.state.reset();
        self.state.set_memory(self.memory.as_ptr());
        self.state.tracer.setup(
            self.data_buffer.as_mut_ptr(),
            u32::try_from(self.data_buffer.len()).unwrap_or(u32::MAX),
            self.pc_buffer.as_mut_ptr(),
            u32::try_from(self.pc_buffer.len()).unwrap_or(u32::MAX),
        );
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
        let Ok(addr) = usize::try_from(addr) else {
            return 0;
        };
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
        let Ok(addr) = usize::try_from(addr) else {
            return 0;
        };
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
}
