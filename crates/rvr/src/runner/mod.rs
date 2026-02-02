//! Runtime execution of compiled RISC-V programs.
//!
//! State management is handled in Rust; only the hot execution loop is in C.
//! Uses trait-based type erasure to support RV32/RV64 × I/E × Tracer variants.

mod api;
mod buffered_diff;
mod debug;
mod diff;
mod error;
mod fixed;
mod preflight;
mod stats;
mod suspend;
mod traits;
mod typed;

use traits::BufferedDiffEntry;

use std::fs::File;
use std::io::{BufReader, BufWriter, Read as IoRead, Write as IoWrite};
use std::path::Path;
use std::time::Instant;

use libloading::os::unix::{Library, RTLD_NOW};
use rvr_elf::{ElfImage, get_elf_xlen};
use rvr_ir::{Rv32, Rv64};
use rvr_isa::{REG_GP, REG_RA, REG_SP};
use rvr_state::{DEFAULT_MEMORY_SIZE, GuardedMemory, NUM_REGS_E, NUM_REGS_I};
use tracing::{debug, error, trace};

pub use api::{FixedAddresses, InstretMode, RvApi, TracerKind};
pub use error::RunError;
pub use traits::RunnerImpl;

use buffered_diff::BufferedDiffRunner;
use debug::DebugRunner;
use diff::DiffRunner;
use fixed::FixedAddrRunner;
use preflight::PreflightRunner;
use stats::StatsRunner;
use suspend::SuspendRunner;
use typed::TypedRunner;

// ============================================================================
// Factory functions
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

/// Create runner implementation with fixed addresses for state and memory.
fn create_fixed_addr_runner(
    elf_data: &[u8],
    fixed: FixedAddresses,
    memory_size: usize,
) -> Result<Box<dyn RunnerImpl>, RunError> {
    let xlen = get_elf_xlen(elf_data)?;

    match xlen {
        32 => {
            let image = ElfImage::<Rv32>::parse(elf_data)?;
            let is_rve = image.is_rve();
            if is_rve {
                Ok(Box::new(FixedAddrRunner::<Rv32, NUM_REGS_E>::new(
                    image,
                    fixed,
                    memory_size,
                )?))
            } else {
                Ok(Box::new(FixedAddrRunner::<Rv32, NUM_REGS_I>::new(
                    image,
                    fixed,
                    memory_size,
                )?))
            }
        }
        64 => {
            let image = ElfImage::<Rv64>::parse(elf_data)?;
            let is_rve = image.is_rve();
            if is_rve {
                Ok(Box::new(FixedAddrRunner::<Rv64, NUM_REGS_E>::new(
                    image,
                    fixed,
                    memory_size,
                )?))
            } else {
                Ok(Box::new(FixedAddrRunner::<Rv64, NUM_REGS_I>::new(
                    image,
                    fixed,
                    memory_size,
                )?))
            }
        }
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
        (TracerKind::Diff, false) => {
            Ok(Box::new(DiffRunner::<Rv32, NUM_REGS_I>::new(image, memory)))
        }
        (TracerKind::Diff, true) => {
            Ok(Box::new(DiffRunner::<Rv32, NUM_REGS_E>::new(image, memory)))
        }
        (TracerKind::BufferedDiff, false) => Ok(Box::new(
            BufferedDiffRunner::<Rv32, NUM_REGS_I>::new(image, memory),
        )),
        (TracerKind::BufferedDiff, true) => Ok(Box::new(
            BufferedDiffRunner::<Rv32, NUM_REGS_E>::new(image, memory),
        )),
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
        (TracerKind::Diff, false) => {
            Ok(Box::new(DiffRunner::<Rv64, NUM_REGS_I>::new(image, memory)))
        }
        (TracerKind::Diff, true) => {
            Ok(Box::new(DiffRunner::<Rv64, NUM_REGS_E>::new(image, memory)))
        }
        (TracerKind::BufferedDiff, false) => Ok(Box::new(
            BufferedDiffRunner::<Rv64, NUM_REGS_I>::new(image, memory),
        )),
        (TracerKind::BufferedDiff, true) => Ok(Box::new(
            BufferedDiffRunner::<Rv64, NUM_REGS_E>::new(image, memory),
        )),
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
    fn setup_initial_regs(&mut self) {
        if let Some(gp) = self.inner.lookup_symbol("__global_pointer$") {
            self.inner.set_register(REG_GP as usize, gp);
        }
        if let Some(sp) = self.inner.lookup_symbol("__stack_top") {
            self.inner.set_register(REG_SP as usize, sp);
        }
        // Trap on unexpected returns from entry points.
        self.inner.set_register(REG_RA as usize, 0);
    }
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

        // Use fixed-address runner if the library was compiled with fixed addresses
        let inner = if let Some(fixed) = api.fixed_addresses {
            debug!(
                state_addr = format!("{:#x}", fixed.state_addr),
                memory_addr = format!("{:#x}", fixed.memory_addr),
                "using fixed addresses"
            );
            create_fixed_addr_runner(&elf_data, fixed, memory_size)?
        } else {
            create_runner_impl(&elf_data, tracer_kind, instret_mode, memory_size)?
        };

        trace!(
            entry_point = format!("{:#x}", inner.entry_point()),
            tracer_kind = ?tracer_kind,
            instret_mode = ?instret_mode,
            memory_size = memory_size,
            fixed_addresses = api.fixed_addresses.is_some(),
            "loaded runner"
        );

        Ok(Self {
            _lib: lib,
            api,
            inner,
        })
    }

    /// Check if library was compiled with export functions mode.
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
    pub fn set_register(&mut self, reg: usize, value: u64) {
        self.inner.set_register(reg, value);
    }

    /// Clear the exit flag to allow further execution.
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
    pub fn get_csr(&self, csr: u16) -> u64 {
        self.inner.get_csr(csr)
    }

    /// Set a CSR (Control and Status Register) value.
    pub fn set_csr(&mut self, csr: u16, value: u64) {
        self.inner.set_csr(csr, value);
    }

    /// Read memory at the given address into the buffer.
    pub fn read_memory(&self, addr: u64, buf: &mut [u8]) -> usize {
        self.inner.read_memory(addr, buf)
    }

    /// Write memory at the given address from the buffer.
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
    pub fn supports_suspend(&self) -> bool {
        self.api.supports_suspend() && self.inner.supports_suspend()
    }

    /// Get the target instret for suspension.
    pub fn get_target_instret(&self) -> Option<u64> {
        self.inner.get_target_instret()
    }

    /// Set the target instret for suspension.
    pub fn set_target_instret(&mut self, target: u64) -> bool {
        self.inner.set_target_instret(target)
    }

    // Diff tracer methods - available when compiled with --tracer diff

    /// Get the PC from the diff tracer (instruction that was just traced).
    pub fn diff_traced_pc(&self) -> Option<u64> {
        self.inner.diff_traced_pc()
    }

    /// Get the opcode from the diff tracer.
    pub fn diff_traced_opcode(&self) -> Option<u32> {
        self.inner.diff_traced_opcode()
    }

    /// Get the destination register if one was written (None for x0 or no write).
    pub fn diff_traced_rd(&self) -> Option<u8> {
        self.inner.diff_traced_rd()
    }

    /// Get the value written to rd.
    pub fn diff_traced_rd_value(&self) -> Option<u64> {
        self.inner.diff_traced_rd_value()
    }

    /// Get memory access info: (addr, value, width, is_write).
    pub fn diff_traced_mem(&self) -> Option<(u64, u64, u8, bool)> {
        self.inner.diff_traced_mem()
    }

    /// Check if diff tracer captured valid state.
    pub fn diff_tracer_valid(&self) -> bool {
        self.inner.diff_tracer_valid()
    }

    // Buffered diff tracer methods - available when compiled with --tracer buffered-diff

    /// Get number of entries captured in the buffered diff tracer.
    pub fn buffered_diff_count(&self) -> Option<usize> {
        self.inner.buffered_diff_count()
    }

    /// Check if buffered diff tracer has overflowed (entries dropped).
    pub fn buffered_diff_has_overflow(&self) -> Option<bool> {
        self.inner.buffered_diff_has_overflow()
    }

    /// Get number of entries dropped due to overflow.
    pub fn buffered_diff_dropped(&self) -> Option<u32> {
        self.inner.buffered_diff_dropped()
    }

    /// Get buffered diff entry at index: (pc, opcode, rd, rd_value, mem_access).
    pub fn buffered_diff_get(&self, index: usize) -> Option<BufferedDiffEntry> {
        self.inner.buffered_diff_get(index)
    }

    /// Reset the buffered diff tracer (clear entries, keep allocation).
    pub fn buffered_diff_reset(&mut self) {
        self.inner.buffered_diff_reset()
    }

    /// Dump register state to stderr for debugging.
    /// Useful for comparing execution between different backends.
    pub fn dump_registers(&self) {
        eprintln!("=== Register State ===");
        eprintln!("pc:      {:016x}", self.inner.get_pc());
        eprintln!("instret: {:016x}", self.inner.instret());
        for i in 0..self.inner.num_regs() {
            eprintln!("x{:02}:     {:016x}", i, self.inner.get_register(i));
        }
    }

    /// Compute a simple checksum of memory for comparison.
    pub fn memory_checksum(&self, start: u64, len: usize) -> u64 {
        let mut checksum: u64 = 0;
        let mut buf = [0u8; 8];
        let mut offset = start;
        let end = start + len as u64;
        while offset < end {
            let read = self.inner.read_memory(offset, &mut buf);
            if read == 0 {
                break;
            }
            checksum = checksum.wrapping_add(u64::from_le_bytes(buf));
            checksum = checksum.rotate_left(7);
            offset += 8;
        }
        checksum
    }

    /// Execute from a specific address.
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
        // Save target_instret before reset (reset() disables the suspender)
        let saved_target = self.inner.get_target_instret();

        self.inner.load_segments();
        self.inner.reset();
        self.setup_initial_regs();

        // Restore target_instret if it was set
        if let Some(target) = saved_target
            && target != u64::MAX
        {
            self.inner.set_target_instret(target);
        }

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

    /// Run multiple times.
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
        self.setup_initial_regs();

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
    pub fn call(&mut self, name: &str, args: &[u64]) -> Result<u64, RunError> {
        let addr = self
            .lookup_symbol(name)
            .ok_or_else(|| RunError::FunctionNotFound(name.to_string()))?;
        self.call_addr(addr, args)
    }

    /// Call a guest function by address with the given arguments.
    pub fn call_addr(&mut self, addr: u64, args: &[u64]) -> Result<u64, RunError> {
        if args.len() > 8 {
            return Err(RunError::TracerSetupFailed(
                "too many arguments (max 8)".to_string(),
            ));
        }

        self.inner.load_segments();
        self.inner.reset();

        // Set up arguments in a0-a7 (registers 10-17)
        for (i, &arg) in args.iter().enumerate() {
            self.inner.set_register(10 + i, arg);
        }

        // Set ra (register 1) to 0 - this will trap when the function returns
        self.inner.set_register(1, 0);

        debug!(addr = format!("{:#x}", addr), "calling guest function");
        unsafe { (self.api.execute_from)(self.inner.as_void_ptr(), addr) };

        Ok(self.inner.get_register(10))
    }

    /// Save the current machine state to a file (zstd compressed).
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

        encoder.write_all(&self.inner.get_pc().to_le_bytes())?;
        encoder.write_all(&self.inner.instret().to_le_bytes())?;

        for i in 0..self.inner.num_regs() {
            encoder.write_all(&self.inner.get_register(i).to_le_bytes())?;
        }

        let mem_size = self.inner.memory_size();
        let mut buf = vec![0u8; 64 * 1024];
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
    pub fn load_state(&mut self, path: impl AsRef<Path>) -> Result<(), RunError> {
        const MAGIC: &[u8; 4] = b"RVR\0";
        const VERSION: u32 = 1;

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

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

        let mut decoder = zstd::stream::Decoder::new(reader)?;

        let mut pc = [0u8; 8];
        decoder.read_exact(&mut pc)?;
        self.inner.set_pc(u64::from_le_bytes(pc));

        let mut instret = [0u8; 8];
        decoder.read_exact(&mut instret)?;

        for i in 0..self.inner.num_regs() {
            let mut reg = [0u8; 8];
            decoder.read_exact(&mut reg)?;
            self.inner.set_register(i, u64::from_le_bytes(reg));
        }

        let mut buf = vec![0u8; 64 * 1024];
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
            self.setup_initial_regs();

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
