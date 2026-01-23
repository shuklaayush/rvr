//! Runtime execution of compiled RISC-V programs.
//!
//! State management is handled in Rust; only the hot execution loop is in C.
//! Uses trait-based type erasure to support RV32/RV64 × I/E × Tracer variants.

use std::ffi::c_void;
use std::path::Path;
use std::time::Instant;

use libloading::os::unix::{Library, Symbol, RTLD_NOW};
use rvr_elf::{get_elf_xlen, ElfImage};
use rvr_ir::{Rv32, Rv64, Xlen};
use rvr_state::{
    DebugTracer, GuardedMemory, PreflightTracer, RvState, StatsTracer, TracerState,
    DEFAULT_MEMORY_SIZE, NUM_REGS_E, NUM_REGS_I,
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
}

/// C API - only the execution function is required.
type RvExecuteFrom = unsafe extern "C" fn(*mut c_void, u64) -> i32;

/// Minimal API from the generated C code.
#[derive(Clone, Copy)]
struct RvApi {
    execute_from: RvExecuteFrom,
    tracer_kind: u32,
    export_functions: bool,
}

impl RvApi {
    unsafe fn load(lib: &Library) -> Result<Self, RunError> {
        Ok(Self {
            execute_from: load_symbol(lib, b"rv_execute_from", "rv_execute_from")?,
            tracer_kind: load_data_symbol(lib, b"RV_TRACER_KIND").unwrap_or(0),
            export_functions: load_data_symbol(lib, b"RV_EXPORT_FUNCTIONS").unwrap_or(0) != 0,
        })
    }
}

unsafe fn load_symbol<T: Copy>(
    lib: &Library,
    symbol: &'static [u8],
    label: &'static str,
) -> Result<T, RunError> {
    let sym: Symbol<T> = lib.get(symbol).map_err(|e| {
        error!(symbol = label, "symbol not found in library");
        RunError::SymbolNotFound(label.to_string(), e)
    })?;
    Ok(*sym)
}

unsafe fn load_data_symbol(lib: &Library, symbol: &'static [u8]) -> Option<u32> {
    let sym: Symbol<*const u32> = lib.get(symbol).ok()?;
    Some(**sym)
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

    /// Clear the exit flag to allow further execution.
    fn clear_exit(&mut self);
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

    fn clear_exit(&mut self) {
        self.state.clear_exit();
    }
}

// ============================================================================
// Factory functions - create the right runner based on ELF and tracer kind
// ============================================================================

/// Create runner implementation based on architecture and tracer.
fn create_runner_impl(
    elf_data: &[u8],
    tracer_kind: TracerKind,
) -> Result<Box<dyn RunnerImpl>, RunError> {
    let memory = GuardedMemory::new(DEFAULT_MEMORY_SIZE)?;
    let xlen = get_elf_xlen(elf_data)?;

    match xlen {
        32 => create_rv32_runner(elf_data, tracer_kind, memory),
        64 => create_rv64_runner(elf_data, tracer_kind, memory),
        _ => unreachable!("get_elf_xlen only returns 32 or 64"),
    }
}

fn create_rv32_runner(
    elf_data: &[u8],
    tracer_kind: TracerKind,
    memory: GuardedMemory,
) -> Result<Box<dyn RunnerImpl>, RunError> {
    let image = ElfImage::<Rv32>::parse(elf_data)?;
    let is_rve = image.is_rve();

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
    memory: GuardedMemory,
) -> Result<Box<dyn RunnerImpl>, RunError> {
    let image = ElfImage::<Rv64>::parse(elf_data)?;
    let is_rve = image.is_rve();

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
    /// Load a compiled shared library and its corresponding ELF.
    pub fn load(lib_dir: impl AsRef<Path>, elf_path: impl AsRef<Path>) -> Result<Self, RunError> {
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

        // Load ELF and create typed runner
        let elf_data = std::fs::read(elf_path)?;
        let inner = create_runner_impl(&elf_data, tracer_kind)?;

        trace!(
            entry_point = format!("{:#x}", inner.entry_point()),
            tracer_kind = ?tracer_kind,
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

    /// Execute from a specific address.
    ///
    /// Call `prepare()` first to load segments and reset state.
    /// Returns the elapsed time and instruction count.
    pub fn execute_from(&mut self, pc: u64) -> (std::time::Duration, u64) {
        let start = Instant::now();
        unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), pc) };
        let elapsed = start.elapsed();
        let instret = self.inner.instret();
        (elapsed, instret)
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
