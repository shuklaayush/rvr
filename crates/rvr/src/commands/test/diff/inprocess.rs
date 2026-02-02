//! In-process executor for differential testing.
//!
//! Uses the compiled C/ARM64 code with instret-based suspension for single-stepping.

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
                Some(DiffState {
                    pc: pc_before,
                    instret: instret_before + 1,
                    is_exit,
                    // Note: opcode, rd, rd_value, mem_addr need tracer support
                    ..Default::default()
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
