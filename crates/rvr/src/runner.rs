//! Runtime execution of compiled RISC-V programs.
//!
//! State management is handled in Rust; only the hot execution loop is in C.
//! Uses trait-based type erasure to support RV32/RV64 × I/E × Tracer variants.

use std::ffi::c_void;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read as IoRead, Write as IoWrite};
use std::path::Path;
use std::time::Instant;

use libloading::os::unix::{Library, RTLD_NOW, Symbol};
use rvr_elf::{ElfImage, get_elf_xlen};
use rvr_ir::{Rv32, Rv64, Xlen};
use rvr_state::{
    DEFAULT_MEMORY_SIZE, DebugTracer, GuardedMemory, InstretSuspender, NUM_REGS_E, NUM_REGS_I,
    PreflightTracer, RvState, StatsTracer, TracerState,
};
use thiserror::Error;
use tracing::{debug, error, trace};

/// Runner error type.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("failed to load library: {0}")]
    LoadError(#[from] libloading::Error),

    #[error("shared library not found: {0}")]
    LibraryNotFound(String),

    #[error("ELF file not found: {0}")]
    ElfNotFound(String),

    #[error("failed to find symbol '{0}': {1}")]
    SymbolNotFound(String, libloading::Error),

    #[error("function not found: {0}")]
    FunctionNotFound(String),

    #[error("memory allocation failed: {0}")]
    MemoryAllocationFailed(#[from] rvr_state::MemoryError),

    #[error("ELF parsing failed: {0}")]
    ElfError(#[from] rvr_elf::ElfError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("execution error: exit code {0}")]
    ExecutionError(u8),

    #[error("tracer setup failed: {0}")]
    TracerSetupFailed(String),

    #[error("state file error: {0}")]
    StateError(String),
}

/// C API - only the execution function is required.
type RvExecuteFrom = unsafe extern "C" fn(*mut c_void, u64) -> i32;

/// Minimal API from the generated C code.
#[derive(Clone, Copy)]
struct RvApi {
    execute_from: RvExecuteFrom,
    tracer_kind: u32,
    export_functions: bool,
    instret_mode: u32,
}

impl RvApi {
    unsafe fn load(lib: &Library) -> Result<Self, RunError> {
        unsafe {
            Ok(Self {
                execute_from: load_symbol(lib, b"rv_execute_from", "rv_execute_from")?,
                tracer_kind: load_data_symbol(lib, b"RV_TRACER_KIND").unwrap_or(0),
                export_functions: load_data_symbol(lib, b"RV_EXPORT_FUNCTIONS").unwrap_or(0) != 0,
                instret_mode: load_data_symbol(lib, b"RV_INSTRET_MODE").unwrap_or(1), // Default to Count
            })
        }
    }

    /// Check if the library supports suspend mode (for single-stepping).
    fn supports_suspend(&self) -> bool {
        self.instret_mode == 2 // Suspend mode
    }
}

unsafe fn load_symbol<T: Copy>(
    lib: &Library,
    symbol: &'static [u8],
    label: &'static str,
) -> Result<T, RunError> {
    unsafe {
        let sym: Symbol<T> = lib.get(symbol).map_err(|e| {
            error!(symbol = label, "symbol not found in library");
            RunError::SymbolNotFound(label.to_string(), e)
        })?;
        Ok(*sym)
    }
}

unsafe fn load_data_symbol(lib: &Library, symbol: &'static [u8]) -> Option<u32> {
    unsafe {
        let sym: Symbol<*const u32> = lib.get(symbol).ok()?;
        Some(**sym)
    }
}

/// Tracer kind matches RV_TRACER_KIND in generated C code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TracerKind {
    None,
    Preflight,
    Stats,
    Ffi,
    Dynamic,
    Debug,
}

impl TracerKind {
    fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Preflight,
            2 => Self::Stats,
            3 => Self::Ffi,
            4 => Self::Dynamic,
            5 => Self::Debug,
            _ => Self::None,
        }
    }
}

/// Instret mode matches RV_INSTRET_MODE in generated C code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstretMode {
    /// No instruction counting.
    Off,
    /// Count instructions but don't suspend.
    Count,
    /// Count instructions and suspend at limit.
    Suspend,
}

impl InstretMode {
    fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Off,
            2 => Self::Suspend,
            _ => Self::Count, // Default to Count (1)
        }
    }

    fn is_suspend(&self) -> bool {
        *self == Self::Suspend
    }
}

const PREFLIGHT_DATA_BYTES: usize = 1 << 20;
const PREFLIGHT_PC_ENTRIES: usize = 1 << 24;
const STATS_ADDR_BITMAP_BYTES: usize = 1 << 29;

// ============================================================================
// RunnerImpl trait - abstracts over XLEN, NUM_REGS, and tracer type
// ============================================================================

/// Trait for type-erased runner implementations.
trait RunnerImpl {
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
    /// Returns None if suspension is not supported.
    fn get_target_instret(&self) -> Option<u64> {
        None
    }

    /// Set the target instret for suspension.
    /// Returns true if successful, false if suspension is not supported.
    fn set_target_instret(&mut self, _target: u64) -> bool {
        false
    }
}

// ============================================================================
// TypedRunner - concrete implementation parameterized by X, T, NUM_REGS
// ============================================================================

/// Typed runner for a specific XLEN, tracer, and register count.
struct TypedRunner<X: Xlen, T: TracerState, const NUM_REGS: usize> {
    state: RvState<X, T, (), NUM_REGS>,
    memory: GuardedMemory,
    elf_image: ElfImage<X>,
}

impl<X: Xlen, T: TracerState, const NUM_REGS: usize> TypedRunner<X, T, NUM_REGS> {
    fn new(elf_image: ElfImage<X>, memory: GuardedMemory) -> Self {
        let mut state = RvState::new();
        state.set_memory(memory.as_ptr());
        Self {
            state,
            memory,
            elf_image,
        }
    }
}

impl<X: Xlen, T: TracerState, const NUM_REGS: usize> RunnerImpl for TypedRunner<X, T, NUM_REGS> {
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
}

// ============================================================================
// TypedRunner with Preflight tracer - needs buffer setup
// ============================================================================

/// Typed runner with preflight tracer (needs buffer management).
struct PreflightRunner<X: Xlen, const NUM_REGS: usize> {
    state: RvState<X, PreflightTracer<X>, (), NUM_REGS>,
    memory: GuardedMemory,
    elf_image: ElfImage<X>,
    data_buffer: Vec<u8>,
    pc_buffer: Vec<X::Reg>,
}

impl<X: Xlen, const NUM_REGS: usize> PreflightRunner<X, NUM_REGS> {
    fn new(elf_image: ElfImage<X>, memory: GuardedMemory) -> Self {
        let mut state = RvState::new();
        state.set_memory(memory.as_ptr());
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
            let vaddr = X::to_u64(seg.virtual_start) as usize;
            unsafe { self.memory.copy_from(vaddr, &seg.data) };
        }
    }

    fn reset(&mut self) {
        self.state.reset();
        self.state.set_memory(self.memory.as_ptr());
        self.state.tracer.setup(
            self.data_buffer.as_mut_ptr(),
            self.data_buffer.len() as u32,
            self.pc_buffer.as_mut_ptr(),
            self.pc_buffer.len() as u32,
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
}

// ============================================================================
// TypedRunner with Stats tracer - needs buffer setup
// ============================================================================

/// Typed runner with stats tracer (needs buffer management).
struct StatsRunner<X: Xlen, const NUM_REGS: usize> {
    state: RvState<X, StatsTracer, (), NUM_REGS>,
    memory: GuardedMemory,
    elf_image: ElfImage<X>,
    addr_bitmap: Vec<u64>,
}

impl<X: Xlen, const NUM_REGS: usize> StatsRunner<X, NUM_REGS> {
    fn new(elf_image: ElfImage<X>, memory: GuardedMemory) -> Self {
        let words = STATS_ADDR_BITMAP_BYTES / 8;
        let mut state = RvState::new();
        state.set_memory(memory.as_ptr());
        Self {
            state,
            memory,
            elf_image,
            addr_bitmap: vec![0u64; words],
        }
    }
}

impl<X: Xlen, const NUM_REGS: usize> RunnerImpl for StatsRunner<X, NUM_REGS> {
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
        self.state.tracer.setup(self.addr_bitmap.as_mut_ptr());
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
}

// ============================================================================
// TypedRunner with Debug tracer - C manages FILE*, Rust provides layout
// ============================================================================

/// Typed runner with debug tracer.
///
/// The debug tracer's FILE* handle is managed by C code (trace_init opens,
/// trace_fini closes). Rust just provides the memory layout for the struct.
struct DebugRunner<X: Xlen, const NUM_REGS: usize> {
    state: RvState<X, DebugTracer, (), NUM_REGS>,
    memory: GuardedMemory,
    elf_image: ElfImage<X>,
}

impl<X: Xlen, const NUM_REGS: usize> DebugRunner<X, NUM_REGS> {
    fn new(elf_image: ElfImage<X>, memory: GuardedMemory) -> Self {
        let mut state = RvState::new();
        state.set_memory(memory.as_ptr());
        Self {
            state,
            memory,
            elf_image,
        }
    }
}

impl<X: Xlen, const NUM_REGS: usize> RunnerImpl for DebugRunner<X, NUM_REGS> {
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
        // DebugTracer's FILE* and pcs are managed by C code (trace_init/trace_fini)
        // We just need to ensure the memory is zeroed
        self.state.tracer = DebugTracer::default();
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
}

// ============================================================================
// SuspendRunner - runner with InstretSuspender for single-stepping
// ============================================================================

/// Typed runner with instret suspension support (for GDB single-stepping).
///
/// Uses `InstretSuspender` instead of `()` for the suspender type parameter,
/// allowing execution to pause after a specific number of instructions.
struct SuspendRunner<X: Xlen, const NUM_REGS: usize> {
    state: RvState<X, (), InstretSuspender, NUM_REGS>,
    memory: GuardedMemory,
    elf_image: ElfImage<X>,
}

impl<X: Xlen, const NUM_REGS: usize> SuspendRunner<X, NUM_REGS> {
    fn new(elf_image: ElfImage<X>, memory: GuardedMemory) -> Self {
        let mut state: RvState<X, (), InstretSuspender, NUM_REGS> = RvState::new();
        state.set_memory(memory.as_ptr());
        // Initialize suspender to not suspend (max u64)
        state.suspender.disable();
        Self {
            state,
            memory,
            elf_image,
        }
    }
}

impl<X: Xlen, const NUM_REGS: usize> RunnerImpl for SuspendRunner<X, NUM_REGS> {
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
}

// ============================================================================
// Factory functions - create the right runner based on ELF and tracer kind
// ============================================================================

/// Create runner implementation based on architecture, tracer, and instret mode.
fn create_runner_impl(
    elf_data: &[u8],
    tracer_kind: TracerKind,
    instret_mode: InstretMode,
    memory_size: usize,
) -> Result<Box<dyn RunnerImpl>, RunError> {
    let memory = GuardedMemory::new(memory_size)?;
    let xlen = get_elf_xlen(elf_data)?;

    match xlen {
        32 => create_rv32_runner(elf_data, tracer_kind, instret_mode, memory),
        64 => create_rv64_runner(elf_data, tracer_kind, instret_mode, memory),
        _ => unreachable!("get_elf_xlen only returns 32 or 64"),
    }
}

fn create_rv32_runner(
    elf_data: &[u8],
    tracer_kind: TracerKind,
    instret_mode: InstretMode,
    memory: GuardedMemory,
) -> Result<Box<dyn RunnerImpl>, RunError> {
    let image = ElfImage::<Rv32>::parse(elf_data)?;
    let is_rve = image.is_rve();

    // Tracers don't currently support suspend mode, so use standard tracers
    match (tracer_kind, is_rve) {
        (TracerKind::Preflight, false) => Ok(Box::new(PreflightRunner::<Rv32, NUM_REGS_I>::new(
            image, memory,
        ))),
        (TracerKind::Preflight, true) => Ok(Box::new(PreflightRunner::<Rv32, NUM_REGS_E>::new(
            image, memory,
        ))),
        (TracerKind::Stats, false) => Ok(Box::new(StatsRunner::<Rv32, NUM_REGS_I>::new(
            image, memory,
        ))),
        (TracerKind::Stats, true) => Ok(Box::new(StatsRunner::<Rv32, NUM_REGS_E>::new(
            image, memory,
        ))),
        (TracerKind::Debug, false) => Ok(Box::new(DebugRunner::<Rv32, NUM_REGS_I>::new(
            image, memory,
        ))),
        (TracerKind::Debug, true) => Ok(Box::new(DebugRunner::<Rv32, NUM_REGS_E>::new(
            image, memory,
        ))),
        // For no tracer, check if suspend mode is requested
        (_, false) if instret_mode.is_suspend() => Ok(Box::new(
            SuspendRunner::<Rv32, NUM_REGS_I>::new(image, memory),
        )),
        (_, true) if instret_mode.is_suspend() => Ok(Box::new(
            SuspendRunner::<Rv32, NUM_REGS_E>::new(image, memory),
        )),
        (_, false) => Ok(Box::new(TypedRunner::<Rv32, (), NUM_REGS_I>::new(
            image, memory,
        ))),
        (_, true) => Ok(Box::new(TypedRunner::<Rv32, (), NUM_REGS_E>::new(
            image, memory,
        ))),
    }
}

fn create_rv64_runner(
    elf_data: &[u8],
    tracer_kind: TracerKind,
    instret_mode: InstretMode,
    memory: GuardedMemory,
) -> Result<Box<dyn RunnerImpl>, RunError> {
    let image = ElfImage::<Rv64>::parse(elf_data)?;
    let is_rve = image.is_rve();

    // Tracers don't currently support suspend mode, so use standard tracers
    match (tracer_kind, is_rve) {
        (TracerKind::Preflight, false) => Ok(Box::new(PreflightRunner::<Rv64, NUM_REGS_I>::new(
            image, memory,
        ))),
        (TracerKind::Preflight, true) => Ok(Box::new(PreflightRunner::<Rv64, NUM_REGS_E>::new(
            image, memory,
        ))),
        (TracerKind::Stats, false) => Ok(Box::new(StatsRunner::<Rv64, NUM_REGS_I>::new(
            image, memory,
        ))),
        (TracerKind::Stats, true) => Ok(Box::new(StatsRunner::<Rv64, NUM_REGS_E>::new(
            image, memory,
        ))),
        (TracerKind::Debug, false) => Ok(Box::new(DebugRunner::<Rv64, NUM_REGS_I>::new(
            image, memory,
        ))),
        (TracerKind::Debug, true) => Ok(Box::new(DebugRunner::<Rv64, NUM_REGS_E>::new(
            image, memory,
        ))),
        // For no tracer, check if suspend mode is requested
        (_, false) if instret_mode.is_suspend() => Ok(Box::new(
            SuspendRunner::<Rv64, NUM_REGS_I>::new(image, memory),
        )),
        (_, true) if instret_mode.is_suspend() => Ok(Box::new(
            SuspendRunner::<Rv64, NUM_REGS_E>::new(image, memory),
        )),
        (_, false) => Ok(Box::new(TypedRunner::<Rv64, (), NUM_REGS_I>::new(
            image, memory,
        ))),
        (_, true) => Ok(Box::new(TypedRunner::<Rv64, (), NUM_REGS_E>::new(
            image, memory,
        ))),
    }
}

// ============================================================================
// Public API types
// ============================================================================

/// Execution result.
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Exit code from the program.
    pub exit_code: u8,
    /// Instruction count (guest instructions retired).
    pub instret: u64,
    /// Wall-clock time in seconds.
    pub time_secs: f64,
    /// Speed in MIPS (million instructions per second).
    pub mips: f64,
}

impl RunResult {
    /// Print result in raw key-value format (for scripting).
    pub fn print_raw_format(&self) {
        println!("instret: {}", self.instret);
        println!("time: {:.6}", self.time_secs);
        println!("mips: {:.2}", self.mips);
    }

    /// Print result in JSON format.
    pub fn print_json(&self) {
        println!(
            r#"{{"instret":{},"time":{:.6},"mips":{:.2},"exit_code":{}}}"#,
            self.instret, self.time_secs, self.mips, self.exit_code
        );
    }
}

/// Hardware performance counters from perf.
#[derive(Debug, Clone, Default)]
pub struct PerfCounters {
    /// Host CPU cycles.
    pub cycles: Option<u64>,
    /// Host instructions executed.
    pub instructions: Option<u64>,
    /// Branch instructions.
    pub branches: Option<u64>,
    /// Branch misses.
    pub branch_misses: Option<u64>,
}

impl PerfCounters {
    /// Calculate instructions per cycle.
    pub fn ipc(&self) -> Option<f64> {
        match (self.instructions, self.cycles) {
            (Some(i), Some(c)) if c > 0 => Some(i as f64 / c as f64),
            _ => None,
        }
    }

    /// Calculate branch miss rate as percentage.
    pub fn branch_miss_rate(&self) -> Option<f64> {
        match (self.branch_misses, self.branches) {
            (Some(m), Some(b)) if b > 0 => Some((m as f64 / b as f64) * 100.0),
            _ => None,
        }
    }
}

/// Execution result with hardware performance counters.
#[derive(Debug, Clone)]
pub struct RunResultWithPerf {
    /// Core execution result.
    pub result: RunResult,
    /// Hardware performance counters (if available).
    pub perf: Option<PerfCounters>,
}

// ============================================================================
// Runner - public API
// ============================================================================

/// Runner for compiled RISC-V programs.
///
/// State is managed entirely in Rust; only the execution loop is in C.
pub struct Runner {
    _lib: Library,
    api: RvApi,
    inner: Box<dyn RunnerImpl>,
}

impl Runner {
    /// Load a compiled shared library and its corresponding ELF with default memory size.
    pub fn load(lib_dir: impl AsRef<Path>, elf_path: impl AsRef<Path>) -> Result<Self, RunError> {
        Self::load_with_memory(lib_dir, elf_path, DEFAULT_MEMORY_SIZE)
    }

    /// Load a compiled shared library and its corresponding ELF with specified memory size.
    pub fn load_with_memory(
        lib_dir: impl AsRef<Path>,
        elf_path: impl AsRef<Path>,
        memory_size: usize,
    ) -> Result<Self, RunError> {
        let lib_dir = lib_dir.as_ref();
        let elf_path = elf_path.as_ref();

        // Derive library name from directory name
        let dir_name = lib_dir.file_name().and_then(|n| n.to_str()).unwrap_or("rv");
        let lib_path = lib_dir.join(format!("lib{}.so", dir_name));

        if !lib_path.exists() {
            error!(path = %lib_path.display(), "shared library not found");
            return Err(RunError::LibraryNotFound(lib_path.display().to_string()));
        }

        if !elf_path.exists() {
            error!(path = %elf_path.display(), "ELF file not found");
            return Err(RunError::ElfNotFound(elf_path.display().to_string()));
        }

        // Load library and API
        // RTLD_NOW is required - RTLD_LAZY causes execution failures because
        // PLT lazy resolution corrupts registers used by preserve_none functions.
        debug!(path = %lib_path.display(), "loading shared library");
        let lib = unsafe { Library::open(Some(&lib_path), RTLD_NOW)? };
        let api = unsafe { RvApi::load(&lib)? };
        let tracer_kind = TracerKind::from_raw(api.tracer_kind);
        let instret_mode = InstretMode::from_raw(api.instret_mode);

        // Load ELF and create typed runner
        let elf_data = std::fs::read(elf_path)?;
        let inner = create_runner_impl(&elf_data, tracer_kind, instret_mode, memory_size)?;

        trace!(
            entry_point = format!("{:#x}", inner.entry_point()),
            tracer_kind = ?tracer_kind,
            instret_mode = ?instret_mode,
            memory_size = memory_size,
            "loaded runner"
        );

        Ok(Self {
            _lib: lib,
            api,
            inner,
        })
    }

    /// Check if library was compiled with export functions mode.
    ///
    /// When enabled, the library exports functions that can be called
    /// independently via `execute_from()` rather than running from entry point.
    pub fn has_export_functions(&self) -> bool {
        self.api.export_functions
    }

    /// Look up a symbol by name and return its address.
    pub fn lookup_symbol(&self, name: &str) -> Option<u64> {
        self.inner.lookup_symbol(name)
    }

    /// Get the entry point address.
    pub fn entry_point(&self) -> u64 {
        self.inner.entry_point()
    }

    /// Load segments and reset state for a fresh run.
    pub fn prepare(&mut self) {
        self.inner.load_segments();
        self.inner.reset();
    }

    /// Set a register value.
    ///
    /// Register 0 (zero) is hardwired to zero and cannot be modified.
    /// Register 1 (ra) is the return address.
    pub fn set_register(&mut self, reg: usize, value: u64) {
        self.inner.set_register(reg, value);
    }

    /// Clear the exit flag to allow further execution.
    ///
    /// After a program exits (e.g., via ebreak), this must be called
    /// before execute_from() can resume execution.
    pub fn clear_exit(&mut self) {
        self.inner.clear_exit();
    }

    /// Get the current instruction count.
    pub fn instret(&self) -> u64 {
        self.inner.instret()
    }

    /// Get the exit code (only valid after program has exited).
    pub fn exit_code(&self) -> u8 {
        self.inner.exit_code()
    }

    /// Get a register value.
    pub fn get_register(&self, reg: usize) -> u64 {
        self.inner.get_register(reg)
    }

    /// Get the program counter.
    pub fn get_pc(&self) -> u64 {
        self.inner.get_pc()
    }

    /// Set the program counter.
    pub fn set_pc(&mut self, pc: u64) {
        self.inner.set_pc(pc);
    }

    /// Get a CSR (Control and Status Register) value.
    ///
    /// Common CSRs:
    /// - 0xC00 (CYCLE): Cycle counter
    /// - 0xC01 (TIME): Wall-clock time (can be set by host)
    /// - 0xC02 (INSTRET): Instructions retired
    pub fn get_csr(&self, csr: u16) -> u64 {
        self.inner.get_csr(csr)
    }

    /// Set a CSR (Control and Status Register) value.
    ///
    /// The TIME CSR (0xC01) can be set to provide wall-clock time to guest programs.
    pub fn set_csr(&mut self, csr: u16, value: u64) {
        self.inner.set_csr(csr, value);
    }

    /// Read memory at the given address into the buffer.
    /// Returns the number of bytes actually read.
    pub fn read_memory(&self, addr: u64, buf: &mut [u8]) -> usize {
        self.inner.read_memory(addr, buf)
    }

    /// Write memory at the given address from the buffer.
    /// Returns the number of bytes actually written.
    pub fn write_memory(&mut self, addr: u64, data: &[u8]) -> usize {
        self.inner.write_memory(addr, data)
    }

    /// Get the number of general-purpose registers (16 for E extension, 32 for I).
    pub fn num_regs(&self) -> usize {
        self.inner.num_regs()
    }

    /// Get the XLEN (32 or 64 bits).
    pub fn xlen(&self) -> u8 {
        self.inner.xlen()
    }

    /// Get the memory size in bytes.
    pub fn memory_size(&self) -> usize {
        self.inner.memory_size()
    }

    /// Check if the runner supports suspend mode (for single-stepping).
    ///
    /// Returns true if both the library was compiled with suspend mode
    /// and the runner was created with InstretSuspender support.
    pub fn supports_suspend(&self) -> bool {
        self.api.supports_suspend() && self.inner.supports_suspend()
    }

    /// Get the target instret for suspension.
    ///
    /// Returns None if suspension is not supported.
    pub fn get_target_instret(&self) -> Option<u64> {
        self.inner.get_target_instret()
    }

    /// Set the target instret for suspension.
    ///
    /// When execution reaches this instret count, it will suspend and return.
    /// Returns true if successful, false if suspension is not supported.
    pub fn set_target_instret(&mut self, target: u64) -> bool {
        self.inner.set_target_instret(target)
    }

    /// Execute from a specific address.
    ///
    /// Call `prepare()` first to load segments and reset state.
    /// Returns `Ok((elapsed, instret))` on success, `Err` if exit code is non-zero.
    pub fn execute_from(&mut self, pc: u64) -> Result<(std::time::Duration, u64), RunError> {
        let start = Instant::now();
        unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), pc) };
        let elapsed = start.elapsed();
        let exit_code = self.inner.exit_code();
        if exit_code != 0 {
            Err(RunError::ExecutionError(exit_code))
        } else {
            Ok((elapsed, self.inner.instret()))
        }
    }

    /// Run the program and return the result.
    pub fn run(&mut self) -> Result<RunResult, RunError> {
        self.inner.load_segments();
        self.inner.reset();

        let entry_point = self.inner.entry_point();
        trace!(entry_point = format!("{:#x}", entry_point), "executing");

        let start = Instant::now();
        unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), entry_point) };
        let elapsed = start.elapsed();

        let instret = self.inner.instret();
        let exit_code = self.inner.exit_code();
        let time_secs = elapsed.as_secs_f64();
        let mips = (instret as f64 / time_secs) / 1_000_000.0;

        trace!(
            instret = instret,
            exit_code = exit_code,
            time_secs = format!("{:.6}", time_secs),
            "execution complete"
        );

        Ok(RunResult {
            exit_code,
            instret,
            time_secs,
            mips,
        })
    }

    /// Run with reset capability for multiple runs.
    pub fn run_multiple(&mut self, count: usize) -> Result<Vec<RunResult>, RunError> {
        let entry_point = self.inner.entry_point();
        let mut results = Vec::with_capacity(count);

        for _ in 0..count {
            self.inner.load_segments();
            self.inner.reset();

            let start = Instant::now();
            unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), entry_point) };
            let elapsed = start.elapsed();

            let instret = self.inner.instret();
            let exit_code = self.inner.exit_code();
            let time_secs = elapsed.as_secs_f64();
            let mips = (instret as f64 / time_secs) / 1_000_000.0;

            results.push(RunResult {
                exit_code,
                instret,
                time_secs,
                mips,
            });
        }

        Ok(results)
    }

    /// Run with hardware performance counters.
    pub fn run_with_counters(&mut self) -> Result<RunResultWithPerf, RunError> {
        self.inner.load_segments();
        self.inner.reset();

        let entry_point = self.inner.entry_point();
        trace!(
            entry_point = format!("{:#x}", entry_point),
            "executing with perf counters"
        );

        let mut perf_group = crate::perf::PerfGroup::new();

        let start = Instant::now();
        if let Some(ref mut group) = perf_group {
            let _ = group.enable();
        }
        unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), entry_point) };
        if let Some(ref mut group) = perf_group {
            let _ = group.disable();
        }
        let elapsed = start.elapsed();

        let instret = self.inner.instret();
        let exit_code = self.inner.exit_code();
        let time_secs = elapsed.as_secs_f64();
        let mips = (instret as f64 / time_secs) / 1_000_000.0;

        let perf = perf_group.as_mut().and_then(|g| g.read());

        let result = RunResult {
            exit_code,
            instret,
            time_secs,
            mips,
        };

        crate::metrics::record_run("unknown", &result, perf.as_ref());

        Ok(RunResultWithPerf { result, perf })
    }

    /// Call a guest function by name with the given arguments.
    ///
    /// Sets up arguments in a0-a7 (registers 10-17) per RISC-V calling convention,
    /// then executes the function. Returns the value in a0 after execution.
    ///
    /// # Requirements
    /// - The library must be compiled with `--export-functions` to have function symbols
    /// - Maximum 8 integer arguments (a0-a7)
    ///
    /// # Example
    /// ```ignore
    /// let result = runner.call("add", &[1, 2])?;
    /// assert_eq!(result, 3);
    /// ```
    pub fn call(&mut self, name: &str, args: &[u64]) -> Result<u64, RunError> {
        let addr = self
            .lookup_symbol(name)
            .ok_or_else(|| RunError::FunctionNotFound(name.to_string()))?;
        self.call_addr(addr, args)
    }

    /// Call a guest function by address with the given arguments.
    ///
    /// Like `call()`, but takes a direct address instead of looking up a symbol.
    pub fn call_addr(&mut self, addr: u64, args: &[u64]) -> Result<u64, RunError> {
        if args.len() > 8 {
            return Err(RunError::TracerSetupFailed(
                "too many arguments (max 8)".to_string(),
            ));
        }

        // Prepare state
        self.inner.load_segments();
        self.inner.reset();

        // Set up arguments in a0-a7 (registers 10-17)
        for (i, &arg) in args.iter().enumerate() {
            self.inner.set_register(10 + i, arg);
        }

        // Set ra (register 1) to 0 - this will trap when the function returns
        self.inner.set_register(1, 0);

        // Execute from function address
        debug!(addr = format!("{:#x}", addr), "calling guest function");
        unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), addr) };

        // Return value is in a0 (register 10)
        let result = self.inner.get_register(10);
        Ok(result)
    }

    /// Save the current machine state to a file (zstd compressed).
    ///
    /// Format: header (uncompressed) + data (zstd compressed)
    /// - Header: magic (4) + version (4) + xlen (1) + num_regs (1) + memory_size (8)
    /// - Data: pc (8) + instret (8) + registers + memory
    pub fn save_state(&self, path: impl AsRef<Path>) -> Result<(), RunError> {
        const MAGIC: &[u8; 4] = b"RVR\0";
        const VERSION: u32 = 1;

        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        // Header (uncompressed)
        writer.write_all(MAGIC)?;
        writer.write_all(&VERSION.to_le_bytes())?;
        writer.write_all(&[self.inner.xlen()])?;
        writer.write_all(&[self.inner.num_regs() as u8])?;
        writer.write_all(&(self.inner.memory_size() as u64).to_le_bytes())?;

        // Data (zstd compressed)
        let mut encoder = zstd::stream::Encoder::new(&mut writer, 3)?;

        // State
        encoder.write_all(&self.inner.get_pc().to_le_bytes())?;
        encoder.write_all(&self.inner.instret().to_le_bytes())?;

        // Registers
        for i in 0..self.inner.num_regs() {
            encoder.write_all(&self.inner.get_register(i).to_le_bytes())?;
        }

        // Memory
        let mem_size = self.inner.memory_size();
        let mut buf = vec![0u8; 64 * 1024]; // 64KB chunks
        let mut offset = 0;
        while offset < mem_size {
            let chunk_size = buf.len().min(mem_size - offset);
            self.inner
                .read_memory(offset as u64, &mut buf[..chunk_size]);
            encoder.write_all(&buf[..chunk_size])?;
            offset += chunk_size;
        }

        encoder.finish()?;
        debug!(size = mem_size, "state saved");
        Ok(())
    }

    /// Load machine state from a file.
    ///
    /// Note: The library must be compatible (same xlen, num_regs, memory_size).
    pub fn load_state(&mut self, path: impl AsRef<Path>) -> Result<(), RunError> {
        const MAGIC: &[u8; 4] = b"RVR\0";
        const VERSION: u32 = 1;

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        // Header (uncompressed)
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(RunError::StateError("invalid state file magic".to_string()));
        }

        let mut version = [0u8; 4];
        reader.read_exact(&mut version)?;
        if u32::from_le_bytes(version) != VERSION {
            return Err(RunError::StateError(
                "unsupported state version".to_string(),
            ));
        }

        let mut xlen = [0u8; 1];
        reader.read_exact(&mut xlen)?;
        if xlen[0] != self.inner.xlen() {
            return Err(RunError::StateError(format!(
                "xlen mismatch: file has {}, runner has {}",
                xlen[0],
                self.inner.xlen()
            )));
        }

        let mut num_regs = [0u8; 1];
        reader.read_exact(&mut num_regs)?;
        if num_regs[0] as usize != self.inner.num_regs() {
            return Err(RunError::StateError(format!(
                "num_regs mismatch: file has {}, runner has {}",
                num_regs[0],
                self.inner.num_regs()
            )));
        }

        let mut mem_size_bytes = [0u8; 8];
        reader.read_exact(&mut mem_size_bytes)?;
        let file_mem_size = u64::from_le_bytes(mem_size_bytes) as usize;
        if file_mem_size != self.inner.memory_size() {
            return Err(RunError::StateError(format!(
                "memory size mismatch: file has {}, runner has {}",
                file_mem_size,
                self.inner.memory_size()
            )));
        }

        // Data (zstd compressed)
        let mut decoder = zstd::stream::Decoder::new(reader)?;

        // State
        let mut pc = [0u8; 8];
        decoder.read_exact(&mut pc)?;
        self.inner.set_pc(u64::from_le_bytes(pc));

        let mut instret = [0u8; 8];
        decoder.read_exact(&mut instret)?;

        // Registers
        for i in 0..self.inner.num_regs() {
            let mut reg = [0u8; 8];
            decoder.read_exact(&mut reg)?;
            self.inner.set_register(i, u64::from_le_bytes(reg));
        }

        // Memory
        let mut buf = vec![0u8; 64 * 1024]; // 64KB chunks
        let mut offset = 0;
        while offset < file_mem_size {
            let chunk_size = buf.len().min(file_mem_size - offset);
            decoder.read_exact(&mut buf[..chunk_size])?;
            self.inner.write_memory(offset as u64, &buf[..chunk_size]);
            offset += chunk_size;
        }

        debug!(size = file_mem_size, "state loaded");
        Ok(())
    }

    /// Run multiple times with hardware performance counters.
    pub fn run_multiple_with_counters(
        &mut self,
        count: usize,
    ) -> Result<RunResultWithPerf, RunError> {
        let entry_point = self.inner.entry_point();
        let mut perf_group = crate::perf::PerfGroup::new();

        let mut total_time = 0.0;
        let mut total_mips = 0.0;
        let mut last_instret = 0;
        let mut last_exit_code = 0;

        for _ in 0..count {
            self.inner.load_segments();
            self.inner.reset();

            if let Some(ref mut group) = perf_group {
                let _ = group.reset();
            }

            let start = Instant::now();
            if let Some(ref mut group) = perf_group {
                let _ = group.enable();
            }
            unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), entry_point) };
            if let Some(ref mut group) = perf_group {
                let _ = group.disable();
            }
            let elapsed = start.elapsed();

            last_instret = self.inner.instret();
            last_exit_code = self.inner.exit_code();
            let time_secs = elapsed.as_secs_f64();
            let mips = (last_instret as f64 / time_secs) / 1_000_000.0;

            total_time += time_secs;
            total_mips += mips;
        }

        let avg_time = total_time / count as f64;
        let avg_mips = total_mips / count as f64;

        let perf = perf_group.as_mut().and_then(|g| g.read());

        let result = RunResult {
            exit_code: last_exit_code,
            instret: last_instret,
            time_secs: avg_time,
            mips: avg_mips,
        };

        crate::metrics::record_run("unknown", &result, perf.as_ref());

        Ok(RunResultWithPerf { result, perf })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_result_format() {
        let result = RunResult {
            exit_code: 0,
            instret: 1_234_567,
            time_secs: 1.234567,
            mips: 1.0,
        };
        result.print_raw_format();
        result.print_json();
    }
}
