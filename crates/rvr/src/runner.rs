//! Runtime execution of compiled RISC-V programs.
//!
//! Uses libloading to load the compiled shared library and call the C API.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::path::Path;
use std::ptr::NonNull;
use std::time::Instant;

use libloading::Library;
use perf_event::events::Hardware;
use perf_event::{Builder, Group};
use thiserror::Error;
use tracing::{debug, error, trace, warn};

/// Runner error type.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("failed to load library: {0}")]
    LoadError(#[from] libloading::Error),

    #[error("shared library not found: {0}")]
    LibraryNotFound(String),

    #[error("failed to find symbol '{0}': {1}")]
    SymbolNotFound(String, libloading::Error),

    #[error("invalid state layout (size {size}, align {align})")]
    InvalidStateLayout { size: usize, align: usize },

    #[error("state allocation failed")]
    StateAllocationFailed,

    #[error("memory initialization failed: {0}")]
    MemoryInitFailed(i32),

    #[error("execution error: exit code {0}")]
    ExecutionError(u8),

    #[error("tracer setup failed: {0}")]
    TracerSetupFailed(String),
}

/// C API function types.
type RvStateSize = unsafe extern "C" fn() -> usize;
type RvStateAlign = unsafe extern "C" fn() -> usize;
type RvStateReset = unsafe extern "C" fn(*mut c_void);
type RvInitMemory = unsafe extern "C" fn(*mut c_void) -> i32;
type RvFreeMemory = unsafe extern "C" fn(*mut c_void);
type RvExecuteFrom = unsafe extern "C" fn(*mut c_void, u32) -> i32;
type RvGetInstret = unsafe extern "C" fn(*const c_void) -> u64;
type RvGetExitCode = unsafe extern "C" fn(*const c_void) -> u8;
type RvGetEntryPoint = unsafe extern "C" fn() -> u32;
type RvTracerPreflightSetup = unsafe extern "C" fn(*mut c_void, *mut u8, u32, *mut c_void, u32);
type RvTracerStatsSetup = unsafe extern "C" fn(*mut c_void, *mut u64);

#[derive(Clone, Copy)]
struct RvApi {
    state_size: RvStateSize,
    state_align: RvStateAlign,
    state_reset: RvStateReset,
    init_memory: RvInitMemory,
    free_memory: RvFreeMemory,
    execute_from: RvExecuteFrom,
    get_instret: RvGetInstret,
    get_exit_code: RvGetExitCode,
    get_entry_point: RvGetEntryPoint,
    reg_bytes: u32,
    tracer_kind: u32,
    tracer_preflight_setup: Option<RvTracerPreflightSetup>,
    tracer_stats_setup: Option<RvTracerStatsSetup>,
}

impl RvApi {
    unsafe fn load(lib: &Library) -> Result<Self, RunError> {
        Ok(Self {
            state_size: load_symbol(lib, b"rv_state_size", "rv_state_size")?,
            state_align: load_symbol(lib, b"rv_state_align", "rv_state_align")?,
            reg_bytes: load_data_symbol(lib, b"RV_REG_BYTES").unwrap_or(0),
            state_reset: load_symbol(lib, b"rv_state_reset", "rv_state_reset")?,
            init_memory: load_symbol(lib, b"rv_init_memory", "rv_init_memory")?,
            free_memory: load_symbol(lib, b"rv_free_memory", "rv_free_memory")?,
            execute_from: load_symbol(lib, b"rv_execute_from", "rv_execute_from")?,
            get_instret: load_symbol(lib, b"rv_get_instret", "rv_get_instret")?,
            get_exit_code: load_symbol(lib, b"rv_get_exit_code", "rv_get_exit_code")?,
            get_entry_point: load_symbol(lib, b"rv_get_entry_point", "rv_get_entry_point")?,
            tracer_kind: load_data_symbol(lib, b"RV_TRACER_KIND").unwrap_or(0),
            tracer_preflight_setup: load_optional_symbol(lib, b"rv_tracer_preflight_setup"),
            tracer_stats_setup: load_optional_symbol(lib, b"rv_tracer_stats_setup"),
        })
    }
}

unsafe fn load_symbol<T: Copy>(
    lib: &Library,
    symbol: &'static [u8],
    label: &'static str,
) -> Result<T, RunError> {
    let sym: libloading::Symbol<T> = lib.get(symbol).map_err(|e| {
        error!(symbol = label, "symbol not found in library");
        RunError::SymbolNotFound(label.to_string(), e)
    })?;
    Ok(*sym)
}

unsafe fn load_data_symbol(lib: &Library, symbol: &'static [u8]) -> Option<u32> {
    let sym: libloading::Symbol<u32> = lib.get(symbol).ok()?;
    Some(*sym)
}

unsafe fn load_optional_symbol<T: Copy>(lib: &Library, symbol: &'static [u8]) -> Option<T> {
    let sym: libloading::Symbol<T> = lib.get(symbol).ok()?;
    Some(*sym)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TracerKind {
    None,
    Preflight,
    Stats,
    Ffi,
    Dynamic,
    Custom,
}

impl TracerKind {
    fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Preflight,
            2 => Self::Stats,
            3 => Self::Ffi,
            4 => Self::Dynamic,
            255 => Self::Custom,
            _ => Self::None,
        }
    }
}

const PREFLIGHT_DATA_BYTES: usize = 1 << 20;
const PREFLIGHT_PC_ENTRIES: usize = 1 << 24;
const STATS_ADDR_BITMAP_BYTES: usize = 1 << 29;

enum PcBuffer {
    U32(Vec<u32>),
    U64(Vec<u64>),
}

impl PcBuffer {
    fn len(&self) -> usize {
        match self {
            Self::U32(buf) => buf.len(),
            Self::U64(buf) => buf.len(),
        }
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        match self {
            Self::U32(buf) => buf.as_mut_ptr() as *mut c_void,
            Self::U64(buf) => buf.as_mut_ptr() as *mut c_void,
        }
    }
}

struct PreflightBuffers {
    data: Vec<u8>,
    pc: PcBuffer,
}

struct StatsBuffers {
    addr_bitmap: Option<Vec<u64>>,
}

struct TracerRuntime {
    kind: TracerKind,
    reg_bytes: u32,
    preflight_setup: Option<RvTracerPreflightSetup>,
    stats_setup: Option<RvTracerStatsSetup>,
    preflight: Option<PreflightBuffers>,
    stats: Option<StatsBuffers>,
}

impl TracerRuntime {
    fn new(api: &RvApi) -> Result<Option<Self>, RunError> {
        let kind = TracerKind::from_raw(api.tracer_kind);
        if kind == TracerKind::None {
            return Ok(None);
        }

        let reg_bytes = api.reg_bytes;
        if reg_bytes == 0 {
            warn!(reg_bytes = reg_bytes, "unsupported register size for tracer");
            return Err(RunError::TracerSetupFailed(format!(
                "unsupported reg size {}",
                reg_bytes
            )));
        }
        debug!(kind = ?kind, reg_bytes = reg_bytes, "initializing tracer");
        Ok(Some(Self {
            kind,
            reg_bytes,
            preflight_setup: api.tracer_preflight_setup,
            stats_setup: api.tracer_stats_setup,
            preflight: None,
            stats: None,
        }))
    }

    fn setup(&mut self, state_ptr: *mut c_void) -> Result<(), RunError> {
        match self.kind {
            TracerKind::Preflight => self.setup_preflight(state_ptr),
            TracerKind::Stats => self.setup_stats(state_ptr),
            _ => Ok(()),
        }
    }

    fn setup_preflight(&mut self, state_ptr: *mut c_void) -> Result<(), RunError> {
        let setup = self.preflight_setup.ok_or_else(|| {
            RunError::TracerSetupFailed("missing rv_tracer_preflight_setup".to_string())
        })?;
        if self.preflight.is_none() {
            let pc = match self.reg_bytes {
                4 => PcBuffer::U32(vec![0u32; PREFLIGHT_PC_ENTRIES]),
                8 => PcBuffer::U64(vec![0u64; PREFLIGHT_PC_ENTRIES]),
                other => {
                    return Err(RunError::TracerSetupFailed(format!(
                        "unsupported reg size {}",
                        other
                    )))
                }
            };
            let data = vec![0u8; PREFLIGHT_DATA_BYTES];
            self.preflight = Some(PreflightBuffers { data, pc });
        }

        let buffers = self.preflight.as_mut().unwrap();
        println!(
            "trace: preflight buffers: data={} bytes, pc={} entries ({} bytes/pc)",
            buffers.data.len(),
            buffers.pc.len(),
            self.reg_bytes
        );
        unsafe {
            setup(
                state_ptr,
                buffers.data.as_mut_ptr(),
                buffers.data.len() as u32,
                buffers.pc.as_mut_ptr(),
                buffers.pc.len() as u32,
            );
        }
        Ok(())
    }

    fn setup_stats(&mut self, state_ptr: *mut c_void) -> Result<(), RunError> {
        let setup = self.stats_setup.ok_or_else(|| {
            RunError::TracerSetupFailed("missing rv_tracer_stats_setup".to_string())
        })?;
        if self.stats.is_none() {
            let words = STATS_ADDR_BITMAP_BYTES / 8;
            let mut addr_bitmap = Vec::new();
            if addr_bitmap.try_reserve_exact(words).is_ok() {
                addr_bitmap.resize(words, 0);
            }
            let addr_bitmap = if addr_bitmap.is_empty() {
                None
            } else {
                Some(addr_bitmap)
            };
            self.stats = Some(StatsBuffers { addr_bitmap });
        }

        let buffers = self.stats.as_mut().unwrap();
        if let Some(addr) = &buffers.addr_bitmap {
            println!("trace: stats addr_bitmap={} bytes", addr.len() * 8);
        } else {
            println!("trace: stats addr_bitmap disabled");
        }
        let addr_ptr = buffers
            .addr_bitmap
            .as_mut()
            .map(|buf| buf.as_mut_ptr())
            .unwrap_or(std::ptr::null_mut());
        unsafe {
            setup(state_ptr, addr_ptr);
        }
        Ok(())
    }
}

struct RunState<'a> {
    ptr: NonNull<c_void>,
    layout: Layout,
    api: &'a RvApi,
    memory_initialized: bool,
    tracer: Option<TracerRuntime>,
}

impl<'a> RunState<'a> {
    fn new(api: &'a RvApi) -> Result<Self, RunError> {
        let size = unsafe { (api.state_size)() };
        let align = unsafe { (api.state_align)() };
        let layout = Layout::from_size_align(size, align).map_err(|_| {
            error!(size = size, align = align, "invalid state layout");
            RunError::InvalidStateLayout { size, align }
        })?;
        let ptr = unsafe { alloc_zeroed(layout) } as *mut c_void;
        let ptr = NonNull::new(ptr).ok_or_else(|| {
            error!(size = size, "state allocation failed");
            RunError::StateAllocationFailed
        })?;
        let tracer = TracerRuntime::new(api)?;
        Ok(Self {
            ptr,
            layout,
            api,
            memory_initialized: false,
            tracer,
        })
    }

    fn reset(&mut self) -> Result<(), RunError> {
        unsafe { (self.api.state_reset)(self.ptr.as_ptr()) };
        if let Some(tracer) = &mut self.tracer {
            tracer.setup(self.ptr.as_ptr())?;
        }
        Ok(())
    }

    fn init_memory(&mut self) -> Result<(), RunError> {
        let rc = unsafe { (self.api.init_memory)(self.ptr.as_ptr()) };
        if rc != 0 {
            error!(rc = rc, "memory initialization failed");
            return Err(RunError::MemoryInitFailed(rc));
        }
        self.memory_initialized = true;
        Ok(())
    }

    fn reinit_memory(&mut self) -> Result<(), RunError> {
        if self.memory_initialized {
            unsafe { (self.api.free_memory)(self.ptr.as_ptr()) };
            self.memory_initialized = false;
        }
        self.init_memory()
    }

    fn execute(&self, entry: u32) {
        unsafe { (self.api.execute_from)(self.ptr.as_ptr(), entry) };
    }

    fn instret(&self) -> u64 {
        unsafe { (self.api.get_instret)(self.ptr.as_ptr()) }
    }

    fn exit_code(&self) -> u8 {
        unsafe { (self.api.get_exit_code)(self.ptr.as_ptr()) }
    }
}

impl Drop for RunState<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.memory_initialized {
                (self.api.free_memory)(self.ptr.as_ptr());
            }
            dealloc(self.ptr.as_ptr() as *mut u8, self.layout);
        }
    }
}

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
    /// Print result in Mojo-compatible format (raw values, no units).
    pub fn print_mojo_format(&self) {
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

/// Runner for compiled RISC-V programs.
pub struct Runner {
    _lib: Library,
    api: RvApi,
}

impl Runner {
    /// Load a compiled shared library.
    pub fn load(lib_dir: impl AsRef<Path>) -> Result<Self, RunError> {
        let lib_dir = lib_dir.as_ref();

        // Derive library name from directory name
        let dir_name = lib_dir.file_name().and_then(|n| n.to_str()).unwrap_or("rv");

        let lib_path = lib_dir.join(format!("lib{}.so", dir_name));

        if !lib_path.exists() {
            error!(path = %lib_path.display(), "shared library not found");
            return Err(RunError::LibraryNotFound(lib_path.display().to_string()));
        }

        debug!(path = %lib_path.display(), "loading shared library");
        let lib = unsafe { Library::new(&lib_path)? };
        let api = unsafe { RvApi::load(&lib)? };
        trace!(
            state_size = unsafe { (api.state_size)() },
            state_align = unsafe { (api.state_align)() },
            "loaded API"
        );
        Ok(Self { _lib: lib, api })
    }

    /// Run the program and return the result.
    pub fn run(&self) -> Result<RunResult, RunError> {
        let mut state = RunState::new(&self.api)?;
        state.init_memory()?;
        state.reset()?;

        let entry_point = unsafe { (self.api.get_entry_point)() };
        trace!(entry_point = format!("{:#x}", entry_point), "executing");

        let start = Instant::now();
        state.execute(entry_point);
        let elapsed = start.elapsed();

        let instret = state.instret();
        let exit_code = state.exit_code();
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
    pub fn run_multiple(&self, count: usize) -> Result<Vec<RunResult>, RunError> {
        let mut state = RunState::new(&self.api)?;
        let entry_point = unsafe { (self.api.get_entry_point)() };
        let mut results = Vec::with_capacity(count);

        for _ in 0..count {
            state.reinit_memory()?;
            state.reset()?;

            let start = Instant::now();
            state.execute(entry_point);
            let elapsed = start.elapsed();

            let instret = state.instret();
            let exit_code = state.exit_code();
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
    /// Returns execution result plus perf counters if available.
    pub fn run_with_counters(&self) -> Result<RunResultWithPerf, RunError> {
        let mut state = RunState::new(&self.api)?;
        state.init_memory()?;
        state.reset()?;

        let entry_point = unsafe { (self.api.get_entry_point)() };
        trace!(entry_point = format!("{:#x}", entry_point), "executing with perf counters");

        // Try to set up perf counters
        let mut perf_group = Self::setup_perf_group();

        let start = Instant::now();

        // Enable counters, execute, disable counters
        if let Some(ref mut group) = perf_group {
            let _ = group.enable();
        }
        state.execute(entry_point);
        if let Some(ref mut group) = perf_group {
            let _ = group.disable();
        }

        let elapsed = start.elapsed();

        let instret = state.instret();
        let exit_code = state.exit_code();
        let time_secs = elapsed.as_secs_f64();
        let mips = (instret as f64 / time_secs) / 1_000_000.0;

        // Read perf counters
        let perf = perf_group.as_mut().and_then(Self::read_perf_counters);

        let result = RunResult {
            exit_code,
            instret,
            time_secs,
            mips,
        };

        // Record metrics
        crate::metrics::record_run("unknown", &result, perf.as_ref());

        Ok(RunResultWithPerf { result, perf })
    }

    /// Run multiple times with hardware performance counters.
    /// Returns averaged results.
    pub fn run_multiple_with_counters(&self, count: usize) -> Result<RunResultWithPerf, RunError> {
        let mut state = RunState::new(&self.api)?;
        let entry_point = unsafe { (self.api.get_entry_point)() };

        let mut perf_group = Self::setup_perf_group();

        let mut total_time = 0.0;
        let mut total_mips = 0.0;
        let mut last_instret = 0;
        let mut last_exit_code = 0;

        for _ in 0..count {
            state.reinit_memory()?;
            state.reset()?;

            if let Some(ref mut group) = perf_group {
                let _ = group.reset();
            }

            let start = Instant::now();
            if let Some(ref mut group) = perf_group {
                let _ = group.enable();
            }
            state.execute(entry_point);
            if let Some(ref mut group) = perf_group {
                let _ = group.disable();
            }
            let elapsed = start.elapsed();

            last_instret = state.instret();
            last_exit_code = state.exit_code();
            let time_secs = elapsed.as_secs_f64();
            let mips = (last_instret as f64 / time_secs) / 1_000_000.0;

            total_time += time_secs;
            total_mips += mips;
        }

        let avg_time = total_time / count as f64;
        let avg_mips = total_mips / count as f64;

        // Read final perf counters (from last run)
        let perf = perf_group.as_mut().and_then(Self::read_perf_counters);

        let result = RunResult {
            exit_code: last_exit_code,
            instret: last_instret,
            time_secs: avg_time,
            mips: avg_mips,
        };

        // Record metrics
        crate::metrics::record_run("unknown", &result, perf.as_ref());

        Ok(RunResultWithPerf { result, perf })
    }

    /// Try to set up a perf event group. Returns None if perf is unavailable.
    fn setup_perf_group() -> Option<PerfGroup> {
        let mut group = Group::new().ok()?;

        // Add counters - store the Counter objects
        let cycles = Builder::new().group(&mut group).kind(Hardware::CPU_CYCLES).build().ok()?;
        let instructions = Builder::new().group(&mut group).kind(Hardware::INSTRUCTIONS).build().ok()?;
        let branches = Builder::new().group(&mut group).kind(Hardware::BRANCH_INSTRUCTIONS).build().ok()?;
        let branch_misses = Builder::new().group(&mut group).kind(Hardware::BRANCH_MISSES).build().ok()?;

        Some(PerfGroup {
            group,
            cycles,
            instructions,
            branches,
            branch_misses,
        })
    }

    /// Read perf counters from a group.
    fn read_perf_counters(perf: &mut PerfGroup) -> Option<PerfCounters> {
        let counts = perf.group.read().ok()?;

        Some(PerfCounters {
            cycles: counts.get(&perf.cycles).copied(),
            instructions: counts.get(&perf.instructions).copied(),
            branches: counts.get(&perf.branches).copied(),
            branch_misses: counts.get(&perf.branch_misses).copied(),
        })
    }
}

/// Helper struct to hold perf event group and counters.
struct PerfGroup {
    group: Group,
    cycles: perf_event::Counter,
    instructions: perf_event::Counter,
    branches: perf_event::Counter,
    branch_misses: perf_event::Counter,
}

impl PerfGroup {
    fn enable(&mut self) -> std::io::Result<()> {
        self.group.enable()
    }

    fn disable(&mut self) -> std::io::Result<()> {
        self.group.disable()
    }

    fn reset(&mut self) -> std::io::Result<()> {
        self.group.reset()
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
        result.print_mojo_format();
        result.print_json();
    }
}
