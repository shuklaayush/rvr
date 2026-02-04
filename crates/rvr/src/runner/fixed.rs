//! `FixedAddrRunner` - runner with state/memory at fixed addresses.

use std::ffi::c_void;

use rvr_elf::ElfImage;
use rvr_ir::Xlen;
use rvr_state::{FixedMemory, GuardedMemory, RvState};

use super::{FixedAddresses, RunError, RunnerImpl};

/// Runner with state and memory allocated at fixed addresses.
///
/// Used when the library was compiled with `--fixed-addresses`. The generated C code
/// expects state and memory at specific addresses and reads them via constexpr casts.
pub struct FixedAddrRunner<X: Xlen, const NUM_REGS: usize> {
    /// Memory region for `RvState` at fixed address.
    state_mem: FixedMemory,
    /// Memory region for guest memory at fixed address.
    memory: GuardedMemory,
    /// ELF image for segment data and symbols.
    elf_image: ElfImage<X>,
}

impl<X: Xlen, const NUM_REGS: usize> FixedAddrRunner<X, NUM_REGS> {
    pub fn new(
        elf_image: ElfImage<X>,
        fixed: FixedAddresses,
        memory_size: usize,
    ) -> Result<Self, RunError> {
        // Allocate state at fixed address
        let state_size = std::mem::size_of::<RvState<X, (), (), NUM_REGS>>();
        let state_mem = FixedMemory::new(fixed.state_addr, state_size)?;

        // Allocate guest memory at fixed address
        let memory = GuardedMemory::new_at_fixed(fixed.memory_addr, memory_size)?;

        // Initialize state in-place
        let state_ptr = state_mem.as_ptr().cast::<RvState<X, (), (), NUM_REGS>>();
        unsafe {
            std::ptr::write(state_ptr, RvState::new());
            (*state_ptr).set_memory(memory.as_ptr());
            let brk = elf_image.get_initial_program_break();
            (*state_ptr).brk = brk;
            (*state_ptr).start_brk = brk;
        }

        Ok(Self {
            state_mem,
            memory,
            elf_image,
        })
    }

    const fn state_ptr(&self) -> *mut RvState<X, (), (), NUM_REGS> {
        self.state_mem
            .as_ptr()
            .cast::<RvState<X, (), (), NUM_REGS>>()
    }

    fn state(&self) -> &RvState<X, (), (), NUM_REGS> {
        unsafe { &*self.state_ptr() }
    }

    fn state_mut(&mut self) -> &mut RvState<X, (), (), NUM_REGS> {
        unsafe { &mut *self.state_ptr() }
    }
}

impl<X: Xlen, const NUM_REGS: usize> RunnerImpl for FixedAddrRunner<X, NUM_REGS> {
    fn load_segments(&mut self) {
        self.memory.clear();
        for seg in &self.elf_image.memory_segments {
            let vaddr = usize::try_from(X::to_u64(seg.virtual_start))
                .expect("segment address does not fit in host usize");
            unsafe { self.memory.copy_from(vaddr, &seg.data) };
        }
    }

    fn reset(&mut self) {
        let state = self.state_mut();
        state.instret = 0;
        state.clear_exit();
        for reg in &mut state.regs {
            *reg = X::from_u64(0);
        }
    }

    fn as_void_ptr(&mut self) -> *mut c_void {
        self.state_mem.as_ptr().cast::<c_void>()
    }

    fn instret(&self) -> u64 {
        self.state().instret
    }

    fn exit_code(&self) -> u8 {
        self.state().exit_code
    }

    fn has_exited(&self) -> bool {
        self.state().has_exited != 0
    }

    fn entry_point(&self) -> u64 {
        X::to_u64(self.elf_image.entry_point)
    }

    fn lookup_symbol(&self, name: &str) -> Option<u64> {
        self.elf_image.lookup_symbol(name)
    }

    fn set_register(&mut self, reg: usize, value: u64) {
        if reg < NUM_REGS && reg != 0 {
            self.state_mut().regs[reg] = X::from_u64(value);
        }
    }

    fn get_register(&self, reg: usize) -> u64 {
        if reg < NUM_REGS {
            X::to_u64(self.state().regs[reg])
        } else {
            0
        }
    }

    fn get_pc(&self) -> u64 {
        X::to_u64(self.state().pc)
    }

    fn set_pc(&mut self, pc: u64) {
        self.state_mut().pc = X::from_u64(pc);
    }

    fn get_csr(&self, _csr: u16) -> u64 {
        0 // CSRs not supported in fixed-address runner
    }

    fn set_csr(&mut self, _csr: u16, _value: u64) {
        // CSRs not supported in fixed-address runner
    }

    fn read_memory(&self, addr: u64, buf: &mut [u8]) -> usize {
        let mem_size = self.memory.size();
        let addr = usize::try_from(addr).expect("address does not fit in host usize");
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
        let addr = usize::try_from(addr).expect("address does not fit in host usize");
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
        self.state_mut().clear_exit();
    }
}
