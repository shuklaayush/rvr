//! Runtime execution of compiled RISC-V programs.
//!
//! Uses libloading to load the compiled shared library and call the C API.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::time::Instant;

use libloading::Library;
use thiserror::Error;

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

    #[error("failed to read metadata: {0}")]
    MetadataReadFailed(String),
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
    tracer_preflight_setup: Option<RvTracerPreflightSetup>,
    tracer_stats_setup: Option<RvTracerStatsSetup>,
}

impl RvApi {
    unsafe fn load(lib: &Library) -> Result<Self, RunError> {
        Ok(Self {
            state_size: load_symbol(lib, b"rv_state_size", "rv_state_size")?,
            state_align: load_symbol(lib, b"rv_state_align", "rv_state_align")?,
            state_reset: load_symbol(lib, b"rv_state_reset", "rv_state_reset")?,
            init_memory: load_symbol(lib, b"rv_init_memory", "rv_init_memory")?,
            free_memory: load_symbol(lib, b"rv_free_memory", "rv_free_memory")?,
            execute_from: load_symbol(lib, b"rv_execute_from", "rv_execute_from")?,
            get_instret: load_symbol(lib, b"rv_get_instret", "rv_get_instret")?,
            get_exit_code: load_symbol(lib, b"rv_get_exit_code", "rv_get_exit_code")?,
            get_entry_point: load_symbol(lib, b"rv_get_entry_point", "rv_get_entry_point")?,
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
    let sym: libloading::Symbol<T> = lib
        .get(symbol)
        .map_err(|e| RunError::SymbolNotFound(label.to_string(), e))?;
    Ok(*sym)
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
    fn from_str(value: &str) -> Self {
        match value {
            "preflight" => Self::Preflight,
            "stats" => Self::Stats,
            "ffi" => Self::Ffi,
            "dynamic" => Self::Dynamic,
            "custom" => Self::Custom,
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

#[derive(Debug, Clone)]
struct RunMetadata {
    xlen: u32,
    tracer_kind: TracerKind,
}

impl RunMetadata {
    fn path(lib_dir: &Path) -> PathBuf {
        lib_dir.join("rvr_meta.json")
    }

    fn read(lib_dir: &Path) -> Result<Option<Self>, RunError> {
        let path = Self::path(lib_dir);
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| RunError::MetadataReadFailed(e.to_string()))?;

        let xlen = extract_u32(&contents, "xlen").unwrap_or(0);
        let tracer_kind = extract_string(&contents, "tracer_kind")
            .map(TracerKind::from_str)
            .unwrap_or(TracerKind::None);

        Ok(Some(Self { xlen, tracer_kind }))
    }
}

fn extract_u32(contents: &str, key: &str) -> Option<u32> {
    let needle = format!("\"{}\":", key);
    let start = contents.find(&needle)? + needle.len();
    let value = contents[start..].trim_start();
    let end = value.find(|c: char| !c.is_ascii_digit()).unwrap_or(value.len());
    value[..end].parse().ok()
}

fn extract_string(contents: &str, key: &str) -> Option<&str> {
    let needle = format!("\"{}\":\"", key);
    let start = contents.find(&needle)? + needle.len();
    let rest = &contents[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

impl TracerRuntime {
    fn new(api: &RvApi, meta: &RunMetadata) -> Result<Option<Self>, RunError> {
        let kind = meta.tracer_kind;
        if kind == TracerKind::None {
            return Ok(None);
        }

        let reg_bytes = match meta.xlen {
            32 => 4,
            64 => 8,
            _ => 0,
        };
        if reg_bytes == 0 {
            return Err(RunError::TracerSetupFailed(format!(
                "unsupported xlen {}",
                meta.xlen
            )));
        }
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
    fn new(api: &'a RvApi, meta: Option<&RunMetadata>) -> Result<Self, RunError> {
        let size = unsafe { (api.state_size)() };
        let align = unsafe { (api.state_align)() };
        let layout = Layout::from_size_align(size, align)
            .map_err(|_| RunError::InvalidStateLayout { size, align })?;
        let ptr = unsafe { alloc_zeroed(layout) } as *mut c_void;
        let ptr = NonNull::new(ptr).ok_or(RunError::StateAllocationFailed)?;
        let tracer = match meta {
            Some(meta) => TracerRuntime::new(api, meta)?,
            None => None,
        };
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
    /// Print result in Mojo-compatible format.
    pub fn print_mojo_format(&self) {
        println!("instret: {}", self.instret);
        println!("time: {:.6}", self.time_secs);
        println!("speed: {:.2} MIPS", self.mips);
    }

    /// Print result in JSON format.
    pub fn print_json(&self) {
        println!(
            r#"{{"instret":{},"time":{:.6},"mips":{:.2},"exit_code":{}}}"#,
            self.instret, self.time_secs, self.mips, self.exit_code
        );
    }
}

/// Runner for compiled RISC-V programs.
pub struct Runner {
    _lib: Library,
    api: RvApi,
    meta: Option<RunMetadata>,
}

impl Runner {
    /// Load a compiled shared library.
    pub fn load(lib_dir: impl AsRef<Path>) -> Result<Self, RunError> {
        let lib_dir = lib_dir.as_ref();

        // Derive library name from directory name
        let dir_name = lib_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rv");

        let lib_path = lib_dir.join(format!("lib{}.so", dir_name));

        if !lib_path.exists() {
            return Err(RunError::LibraryNotFound(lib_path.display().to_string()));
        }

        let meta = RunMetadata::read(lib_dir)?;
        let lib = unsafe { Library::new(&lib_path)? };
        let api = unsafe { RvApi::load(&lib)? };
        Ok(Self {
            _lib: lib,
            api,
            meta,
        })
    }

    /// Run the program and return the result.
    pub fn run(&self) -> Result<RunResult, RunError> {
        let mut state = RunState::new(&self.api, self.meta.as_ref())?;
        state.init_memory()?;
        state.reset()?;

        let entry_point = unsafe { (self.api.get_entry_point)() };

        let start = Instant::now();
        state.execute(entry_point);
        let elapsed = start.elapsed();

        let instret = state.instret();
        let exit_code = state.exit_code();
        let time_secs = elapsed.as_secs_f64();
        let mips = (instret as f64 / time_secs) / 1_000_000.0;

        Ok(RunResult {
            exit_code,
            instret,
            time_secs,
            mips,
        })
    }

    /// Run with reset capability for multiple runs.
    pub fn run_multiple(&self, count: usize) -> Result<Vec<RunResult>, RunError> {
        let mut state = RunState::new(&self.api, self.meta.as_ref())?;
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
