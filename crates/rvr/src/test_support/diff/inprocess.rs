//! In-process executor for differential testing.
//!
//! Provides two executor types:
//! - `InProcessExecutor`: Single-steps using diff tracer (linear mode)
//! - `BufferedInProcessExecutor`: Runs blocks with buffered diff tracer (block mode)

use std::path::Path;

use crate::{RunError, Runner};

use super::executor::Executor;
use super::state::DiffState;

/// In-process executor using compiled rvr code.
///
/// Uses `SuspendRunner` with instret stepping to execute one instruction at a time.
/// The tracer captures register writes and memory accesses for comparison.
pub struct InProcessExecutor {
    runner: Runner,
    has_exited: bool,
}

impl InProcessExecutor {
    /// Create a new in-process executor from a compiled library directory.
    pub fn new(lib_dir: &Path, elf_path: &Path) -> Result<Self, RunError> {
        let mut runner = Runner::load(lib_dir, elf_path)?;

        // Verify the library supports suspend mode
        if !runner.supports_suspend() {
            return Err(RunError::ExecutionError(255));
        }

        // Prepare for execution
        runner.prepare();

        // Set PC to entry point (reset() sets it to 0)
        let entry_point = runner.entry_point();
        runner.set_pc(entry_point);

        Ok(Self {
            runner,
            has_exited: false,
        })
    }
}

impl Executor for InProcessExecutor {
    fn step(&mut self) -> Option<DiffState> {
        if self.has_exited {
            return None;
        }

        // Capture PC BEFORE execution (to match Spike's trace format)
        let pc_before = self.runner.get_pc();
        let instret_before = self.runner.instret();

        let current = self.runner.instret();
        self.runner.set_target_instret(current + 1);
        self.runner.clear_exit();

        match self.runner.execute_from(pc_before) {
            Ok(_) => {
                let is_exit = self.runner.exit_code() != 0;
                if is_exit {
                    self.has_exited = true;
                }

                // Capture state from diff tracer if available
                let opcode = self.runner.diff_traced_opcode().unwrap_or(0);
                let rd = self.runner.diff_traced_rd();
                let rd_value = self.runner.diff_traced_rd_value();
                let (mem_addr, mem_value, mem_width, is_write) = self.runner.diff_traced_mem().map_or(
                    (None::<u64>, None::<u64>, None::<u8>, false),
                    |(addr, val, width, is_write)| (Some(addr), Some(val), Some(width), is_write),
                );

                Some(DiffState {
                    pc: pc_before,
                    opcode,
                    instret: instret_before + 1,
                    rd,
                    rd_value,
                    mem_addr,
                    mem_value,
                    mem_width,
                    is_write,
                    is_exit,
                })
            }
            Err(_) => {
                // Execution error - check if it's a normal exit
                self.has_exited = true;
                if self.runner.exit_code() == 0 {
                    None
                } else {
                    Some(DiffState {
                        pc: pc_before,
                        instret: instret_before + 1,
                        is_exit: true,
                        ..Default::default()
                    })
                }
            }
        }
    }
}

/// In-process executor with buffered diff tracer for block-level comparison.
///
/// Runs blocks (not single-stepping) and captures instruction states in a ring buffer.
/// After each block, the captured entries can be iterated and compared against a
/// linear executor's step-by-step results.
pub struct BufferedInProcessExecutor {
    runner: Runner,
    has_exited: bool,
    /// Current index into the captured buffer during iteration.
    buffer_index: usize,
}

impl BufferedInProcessExecutor {
    /// Create a new buffered in-process executor from a compiled library directory.
    ///
    /// The library must have been compiled with `--tracer buffered-diff`.
    pub fn new(lib_dir: &Path, elf_path: &Path) -> Result<Self, RunError> {
        let mut runner = Runner::load(lib_dir, elf_path)?;

        // Verify the library has buffered diff tracer support
        if runner.buffered_diff_count().is_none() {
            return Err(RunError::TracerSetupFailed(
                "library not compiled with buffered-diff tracer".to_string(),
            ));
        }

        // Prepare for execution
        runner.prepare();

        // Set PC to entry point
        let entry_point = runner.entry_point();
        runner.set_pc(entry_point);

        Ok(Self {
            runner,
            has_exited: false,
            buffer_index: 0,
        })
    }

    /// Run until the next block boundary or exit.
    ///
    /// Returns the number of instructions captured in the buffer.
    /// Returns 0 if the program has exited.
    pub fn run_block(&mut self) -> usize {
        if self.has_exited {
            return 0;
        }

        // Reset buffer for fresh capture
        self.runner.buffered_diff_reset();
        self.buffer_index = 0;

        let pc = self.runner.get_pc();
        self.runner.clear_exit();

        match self.runner.execute_from(pc) {
            Ok(_) => {
                if self.runner.exit_code() != 0 {
                    self.has_exited = true;
                }
                self.runner.buffered_diff_count().unwrap_or(0)
            }
            Err(_) => {
                self.has_exited = true;
                self.runner.buffered_diff_count().unwrap_or(0)
            }
        }
    }

    /// Get the number of entries captured in the buffer.
    pub fn captured_count(&self) -> usize {
        self.runner.buffered_diff_count().unwrap_or(0)
    }

    /// Check if any entries were dropped due to buffer overflow.
    pub fn has_overflow(&self) -> bool {
        self.runner.buffered_diff_has_overflow().unwrap_or(false)
    }

    /// Get the number of entries dropped due to overflow.
    pub fn dropped_count(&self) -> u32 {
        self.runner.buffered_diff_dropped().unwrap_or(0)
    }

    /// Get entry at index from the capture buffer.
    pub fn get_entry(&self, index: usize) -> Option<DiffState> {
        let (pc, opcode, rd, rd_value, mem_access) = self.runner.buffered_diff_get(index)?;
        let (mem_addr, mem_value, mem_width, is_write) = mem_access.map_or(
            (None::<u64>, None::<u64>, None::<u8>, false),
            |(addr, val, width, is_write)| (Some(addr), Some(val), Some(width), is_write),
        );
        Some(DiffState {
            pc,
            opcode,
            instret: 0, // Not tracked per-entry in buffer
            rd,
            rd_value,
            mem_addr,
            mem_value,
            mem_width,
            is_write,
            is_exit: false,
        })
    }

    /// Check if the program has exited.
    pub fn has_exited(&self) -> bool {
        self.has_exited
    }
}

/// Iterator adapter for BufferedInProcessExecutor that yields DiffStates one at a time.
///
/// This allows using BufferedInProcessExecutor through the Executor trait interface
/// for block-level comparison.
impl Executor for BufferedInProcessExecutor {
    fn step(&mut self) -> Option<DiffState> {
        // If we have buffered entries, return the next one
        if self.buffer_index < self.captured_count() {
            let state = self.get_entry(self.buffer_index);
            self.buffer_index += 1;
            return state;
        }

        // If exited, no more states
        if self.has_exited {
            return None;
        }

        // Run a block to capture more entries
        let count = self.run_block();
        if count == 0 {
            return None;
        }

        // Return first entry
        self.buffer_index = 1;
        self.get_entry(0)
    }
}
