//! In-process executor for differential testing.
//!
//! Provides two executor types:
//! - `InProcessExecutor`: Single-steps using diff tracer (linear mode)
//! - `BufferedInProcessExecutor`: Runs blocks with buffered diff tracer (block mode)

use std::path::Path;

use rvr::{RunError, Runner};

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

    /// Capture the current state after an instruction.
    fn capture_state(&self) -> DiffState {
        DiffState {
            pc: self.runner.get_pc(),
            instret: self.runner.instret(),
            is_exit: self.runner.exit_code() != 0,
            // Note: opcode, rd, rd_value, mem_addr are captured via tracer
            // For now, we only compare PC and instret in basic mode
            ..Default::default()
        }
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
                let (mem_addr, mem_value, mem_width, is_write) = self
                    .runner
                    .diff_traced_mem()
                    .map_or((None, None, None, false), |(addr, val, width, is_write)| {
                        (Some(addr), Some(val), Some(width), is_write)
                    });

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

    fn step_n(&mut self, n: u64) -> Option<DiffState> {
        if self.has_exited || n == 0 {
            return None;
        }

        let current = self.runner.instret();
        self.runner.set_target_instret(current + n);
        self.runner.clear_exit();

        match self.runner.execute_from(self.runner.get_pc()) {
            Ok(_) => {
                let state = self.capture_state();
                if state.is_exit {
                    self.has_exited = true;
                }
                Some(state)
            }
            Err(_) => {
                let state = self.capture_state();
                self.has_exited = true;
                if self.runner.exit_code() == 0 {
                    None
                } else {
                    Some(DiffState {
                        is_exit: true,
                        ..state
                    })
                }
            }
        }
    }

    fn current_pc(&self) -> u64 {
        self.runner.get_pc()
    }

    fn instret(&self) -> u64 {
        self.runner.instret()
    }

    fn has_exited(&self) -> bool {
        self.has_exited
    }

    fn exit_code(&self) -> Option<u8> {
        if self.has_exited {
            Some(self.runner.exit_code())
        } else {
            None
        }
    }

    fn reset_to(&mut self, pc: u64) -> bool {
        self.runner.set_pc(pc);
        self.runner.clear_exit();
        self.has_exited = false;
        true
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

    /// Run N instructions and capture their states.
    ///
    /// Uses instret-based suspension if supported.
    /// Returns the number of instructions captured in the buffer.
    pub fn run_n(&mut self, n: u64) -> usize {
        if self.has_exited || n == 0 {
            return 0;
        }

        // Reset buffer for fresh capture
        self.runner.buffered_diff_reset();
        self.buffer_index = 0;

        let current = self.runner.instret();
        if self.runner.supports_suspend() {
            self.runner.set_target_instret(current + n);
        }

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
        let (mem_addr, mem_value, mem_width, is_write) = mem_access
            .map_or((None, None, None, false), |(addr, val, width, is_write)| {
                (Some(addr), Some(val), Some(width), is_write)
            });
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

    /// Iterate over all captured entries as DiffStates.
    pub fn iter_entries(&self) -> impl Iterator<Item = DiffState> + '_ {
        (0..self.captured_count()).filter_map(|i| self.get_entry(i))
    }

    /// Get the current PC.
    pub fn current_pc(&self) -> u64 {
        self.runner.get_pc()
    }

    /// Get the current instruction count.
    pub fn instret(&self) -> u64 {
        self.runner.instret()
    }

    /// Check if the program has exited.
    pub fn has_exited(&self) -> bool {
        self.has_exited
    }

    /// Get the exit code if exited.
    pub fn exit_code(&self) -> Option<u8> {
        if self.has_exited {
            Some(self.runner.exit_code())
        } else {
            None
        }
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

    fn step_n(&mut self, n: u64) -> Option<DiffState> {
        // Run N instructions
        let count = self.run_n(n);
        if count == 0 {
            return None;
        }

        // Return last entry
        self.get_entry(count.saturating_sub(1))
    }

    fn current_pc(&self) -> u64 {
        self.runner.get_pc()
    }

    fn instret(&self) -> u64 {
        self.runner.instret()
    }

    fn has_exited(&self) -> bool {
        self.has_exited
    }

    fn exit_code(&self) -> Option<u8> {
        if self.has_exited {
            Some(self.runner.exit_code())
        } else {
            None
        }
    }

    fn reset_to(&mut self, pc: u64) -> bool {
        self.runner.set_pc(pc);
        self.runner.clear_exit();
        self.has_exited = false;
        self.buffer_index = 0;
        self.runner.buffered_diff_reset();
        true
    }
}
