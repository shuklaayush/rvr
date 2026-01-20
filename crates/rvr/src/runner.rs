//! Runtime execution of compiled RISC-V programs.
//!
//! Uses libloading to load the compiled shared library and call the C API.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::path::Path;
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

struct RunState<'a> {
    ptr: NonNull<c_void>,
    layout: Layout,
    api: &'a RvApi,
    memory_initialized: bool,
}

impl<'a> RunState<'a> {
    fn new(api: &'a RvApi) -> Result<Self, RunError> {
        let size = unsafe { (api.state_size)() };
        let align = unsafe { (api.state_align)() };
        let layout = Layout::from_size_align(size, align)
            .map_err(|_| RunError::InvalidStateLayout { size, align })?;
        let ptr = unsafe { alloc_zeroed(layout) } as *mut c_void;
        let ptr = NonNull::new(ptr).ok_or(RunError::StateAllocationFailed)?;
        Ok(Self {
            ptr,
            layout,
            api,
            memory_initialized: false,
        })
    }

    fn reset(&mut self) {
        unsafe { (self.api.state_reset)(self.ptr.as_ptr()) };
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

        let lib = unsafe { Library::new(&lib_path)? };
        let api = unsafe { RvApi::load(&lib)? };
        Ok(Self { _lib: lib, api })
    }

    /// Run the program and return the result.
    pub fn run(&self) -> Result<RunResult, RunError> {
        let mut state = RunState::new(&self.api)?;
        state.init_memory()?;
        state.reset();

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
        let mut state = RunState::new(&self.api)?;
        let entry_point = unsafe { (self.api.get_entry_point)() };
        let mut results = Vec::with_capacity(count);

        for _ in 0..count {
            state.reinit_memory()?;
            state.reset();

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
