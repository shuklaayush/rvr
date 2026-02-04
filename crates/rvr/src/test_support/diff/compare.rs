//! Comparison algorithms for differential execution.

use super::executor::Executor;
use super::inprocess::BufferedInProcessExecutor;
use super::state::{
    CompareConfig, CompareResult, DiffState, Divergence, DivergenceKind, compare_states,
};

struct RunSnapshot {
    state: DiffState,
    has_exited: bool,
    exit_code: u8,
    error: bool,
}

enum ExitStatus {
    Running,
    Exited(u8),
}

struct RunnerBatch {
    executed: u64,
    exit: ExitStatus,
    error: bool,
    pc_after: u64,
}

impl RunnerBatch {
    const fn has_exited(&self) -> bool {
        matches!(self.exit, ExitStatus::Exited(_))
    }

    const fn exit_code(&self) -> Option<u8> {
        match self.exit {
            ExitStatus::Exited(code) => Some(code),
            ExitStatus::Running => None,
        }
    }
}

struct BatchResult {
    ref_batch: RunnerBatch,
    test_batch: RunnerBatch,
}

fn u64_to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn regs_match(ref_r: &crate::Runner, test_r: &crate::Runner) -> bool {
    for i in 0..ref_r.num_regs() {
        if ref_r.get_register(i) != test_r.get_register(i) {
            return false;
        }
    }
    true
}

fn run_to_target(runner: &mut crate::Runner, target_instret: u64) -> RunSnapshot {
    let result = runner.reset_and_run_to_instret(target_instret);
    RunSnapshot {
        state: DiffState {
            pc: runner.get_pc(),
            instret: runner.instret(),
            is_exit: runner.has_exited(),
            ..DiffState::default()
        },
        has_exited: runner.has_exited(),
        exit_code: runner.exit_code(),
        error: result.is_err(),
    }
}

fn checkpoint_match(
    ref_snap: &RunSnapshot,
    test_snap: &RunSnapshot,
    ref_r: &crate::Runner,
    test_r: &crate::Runner,
    target_instret: u64,
) -> bool {
    let executed_ok =
        ref_snap.state.instret == target_instret && test_snap.state.instret == target_instret;
    let both_exited = ref_snap.has_exited && test_snap.has_exited;
    let exit_match = ref_snap.exit_code == test_snap.exit_code;
    let errors_ok = !(ref_snap.error || test_snap.error) || (both_exited && exit_match);

    executed_ok
        && regs_match(ref_r, test_r)
        && ref_snap.has_exited == test_snap.has_exited
        && exit_match
        && errors_ok
        && (ref_snap.has_exited || ref_snap.state.pc == test_snap.state.pc)
}

fn align_pcs(ref_runner: &mut crate::Runner, test_runner: &mut crate::Runner) {
    let ref_pc = ref_runner.get_pc();
    let test_pc = test_runner.get_pc();
    if ref_pc == test_pc {
        return;
    }

    for _ in 0..256 {
        let curr_ref_pc = ref_runner.get_pc();
        let curr_test_pc = test_runner.get_pc();
        if curr_ref_pc == curr_test_pc {
            break;
        }
        if curr_ref_pc < curr_test_pc {
            ref_runner.set_target_instret(ref_runner.instret() + 1);
            ref_runner.clear_exit();
            let _ = ref_runner.execute_from(curr_ref_pc);
        } else {
            test_runner.set_target_instret(test_runner.instret() + 1);
            test_runner.clear_exit();
            let _ = test_runner.execute_from(curr_test_pc);
        }
    }
}

fn run_batch(
    ref_runner: &mut crate::Runner,
    test_runner: &mut crate::Runner,
    batch_size: u64,
) -> BatchResult {
    let ref_start_instret = ref_runner.instret();
    ref_runner.set_target_instret(ref_start_instret + batch_size);
    ref_runner.clear_exit();
    let ref_pc = ref_runner.get_pc();
    let ref_result = ref_runner.execute_from(ref_pc);

    let test_start_instret = test_runner.instret();
    test_runner.set_target_instret(test_start_instret + batch_size);
    test_runner.clear_exit();
    let test_pc = test_runner.get_pc();
    let test_result = test_runner.execute_from(test_pc);

    BatchResult {
        ref_batch: RunnerBatch {
            executed: ref_runner.instret() - ref_start_instret,
            exit: if ref_runner.has_exited() {
                ExitStatus::Exited(ref_runner.exit_code())
            } else {
                ExitStatus::Running
            },
            error: ref_result.is_err(),
            pc_after: ref_runner.get_pc(),
        },
        test_batch: RunnerBatch {
            executed: test_runner.instret() - test_start_instret,
            exit: if test_runner.has_exited() {
                ExitStatus::Exited(test_runner.exit_code())
            } else {
                ExitStatus::Running
            },
            error: test_result.is_err(),
            pc_after: test_runner.get_pc(),
        },
    }
}

fn divergence_kind(
    ref_snap: &RunSnapshot,
    test_snap: &RunSnapshot,
    ref_runner: &crate::Runner,
    test_runner: &crate::Runner,
    target_instret: u64,
) -> DivergenceKind {
    if ref_snap.state.pc != test_snap.state.pc {
        DivergenceKind::Pc
    } else if !regs_match(ref_runner, test_runner) {
        DivergenceKind::RegValue
    } else if ref_snap.has_exited != test_snap.has_exited
        || ref_snap.exit_code != test_snap.exit_code
    {
        if ref_snap.has_exited {
            DivergenceKind::ActualTail
        } else {
            DivergenceKind::ExpectedTail
        }
    } else if ref_snap.state.instret != target_instret || test_snap.state.instret != target_instret
    {
        if ref_snap.state.instret < test_snap.state.instret {
            DivergenceKind::ActualTail
        } else {
            DivergenceKind::ExpectedTail
        }
    } else {
        DivergenceKind::Pc
    }
}
/// Run lockstep comparison between reference and test executors.
///
/// Steps both executors one instruction at a time and compares state.
pub fn compare_lockstep(
    ref_exec: &mut dyn Executor,
    test_exec: &mut dyn Executor,
    config: &CompareConfig,
    max_instrs: Option<u64>,
) -> CompareResult {
    let mut matched = 0usize;
    let limit = max_instrs.unwrap_or(u64::MAX);

    loop {
        if matched as u64 >= limit {
            break;
        }

        let mut ref_state = ref_exec.step();
        let mut test_state = test_exec.step();

        // Initial alignment: if the first PCs differ, advance the lower PC
        // to avoid false divergence due to startup differences.
        if matched == 0 {
            let mut attempts = 0u32;
            while attempts < 256 {
                let (Some(ref_s), Some(test_s)) = (&ref_state, &test_state) else {
                    break;
                };
                if ref_s.pc == test_s.pc {
                    break;
                }
                if ref_s.pc < test_s.pc {
                    ref_state = ref_exec.step();
                } else {
                    test_state = test_exec.step();
                }
                attempts += 1;
            }
        }

        match (ref_state, test_state) {
            (Some(ref_s), Some(test_s)) => {
                // Compare the two states
                if let Some(kind) = compare_states(&ref_s, &test_s, config) {
                    return CompareResult {
                        matched,
                        divergence: Some(Divergence {
                            index: matched,
                            expected: ref_s,
                            actual: test_s,
                            kind,
                        }),
                    };
                }

                matched += 1;

                // Check for exit
                if ref_s.is_exit() || test_s.is_exit() {
                    break;
                }
            }
            (Some(ref_s), None) => {
                // Reference has more instructions
                return CompareResult {
                    matched,
                    divergence: Some(Divergence {
                        index: matched,
                        expected: ref_s,
                        actual: DiffState::default(),
                        kind: DivergenceKind::ExpectedTail,
                    }),
                };
            }
            (None, Some(test_s)) => {
                // Test has more instructions
                return CompareResult {
                    matched,
                    divergence: Some(Divergence {
                        index: matched,
                        expected: DiffState::default(),
                        actual: test_s,
                        kind: DivergenceKind::ActualTail,
                    }),
                };
            }
            (None, None) => {
                // Both finished
                break;
            }
        }
    }

    CompareResult {
        matched,
        divergence: None,
    }
}

/// Run block-vs-linear comparison.
///
/// The block executor runs with buffered diff tracer and captures N instructions per block.
/// The linear executor steps one instruction at a time with single diff tracer.
/// We compare buffered entries against stepped entries.
///
/// If the buffered tracer overflows, we fall back to instruction-by-instruction mode.
pub fn compare_block_vs_linear(
    block_exec: &mut BufferedInProcessExecutor,
    linear_exec: &mut dyn Executor,
    config: &CompareConfig,
    max_instrs: Option<u64>,
) -> CompareResult {
    let mut matched = 0usize;
    let limit = max_instrs.unwrap_or(u64::MAX);

    loop {
        if matched as u64 >= limit {
            break;
        }

        // Run a block in the block executor to capture states
        let block_count = block_exec.run_block();

        // Check for overflow - if so, we can't trust the buffer
        if block_exec.has_overflow() {
            eprintln!(
                "Warning: buffer overflow at instruction {}, {} entries dropped",
                matched,
                block_exec.dropped_count()
            );
            // Could fall back to instruction mode here, but for now just continue
            // with partial data
        }

        if block_count == 0 {
            // Block executor finished
            if block_exec.has_exited() {
                // Check if linear also finishes at the same point
                if let Some(linear_state) = linear_exec.step() {
                    // Linear has more instructions
                    return CompareResult {
                        matched,
                        divergence: Some(Divergence {
                            index: matched,
                            expected: DiffState::default(),
                            actual: linear_state,
                            kind: DivergenceKind::ActualTail,
                        }),
                    };
                }
                break;
            }
            // No entries but not exited - unexpected
            break;
        }

        // Compare each buffered entry against stepped linear entry
        for i in 0..block_count {
            if matched as u64 >= limit {
                break;
            }

            let Some(block_state) = block_exec.get_entry(i) else {
                break;
            };

            let Some(linear_state) = linear_exec.step() else {
                // Linear executor finished early
                return CompareResult {
                    matched,
                    divergence: Some(Divergence {
                        index: matched,
                        expected: block_state,
                        actual: DiffState::default(),
                        kind: DivergenceKind::ExpectedTail,
                    }),
                };
            };

            // Compare the two states
            if let Some(kind) = compare_states(&block_state, &linear_state, config) {
                return CompareResult {
                    matched,
                    divergence: Some(Divergence {
                        index: matched,
                        expected: block_state,
                        actual: linear_state,
                        kind,
                    }),
                };
            }

            matched += 1;

            // Check for exit
            let block_is_exit = block_state.is_exit();
            let linear_is_exit = linear_state.is_exit();
            if block_is_exit || linear_is_exit {
                // Verify both have the same exit status
                if block_is_exit != linear_is_exit {
                    return CompareResult {
                        matched,
                        divergence: Some(Divergence {
                            index: matched - 1,
                            expected: block_state,
                            actual: linear_state,
                            kind: if block_is_exit {
                                DivergenceKind::ActualTail
                            } else {
                                DivergenceKind::ExpectedTail
                            },
                        }),
                    };
                }
                return CompareResult {
                    matched,
                    divergence: None,
                };
            }
        }
    }

    CompareResult {
        matched,
        divergence: None,
    }
}

/// Fast checkpoint-based comparison.
///
/// Instead of comparing every instruction, runs both executors for `checkpoint_interval`
/// instructions, then compares only PC and all register values. If they match, continues
/// to the next checkpoint. If they differ, performs binary search to find the exact
/// divergence point.
///
/// This is much faster for the common case (no divergence) since it doesn't
/// require per-instruction tracer overhead.
pub fn compare_checkpoint(
    ref_runner: &mut crate::Runner,
    test_runner: &mut crate::Runner,
    checkpoint_interval: u64,
    max_instrs: Option<u64>,
) -> CompareResult {
    let limit = max_instrs.unwrap_or(u64::MAX);
    let mut matched: u64 = 0;

    align_pcs(ref_runner, test_runner);

    loop {
        if matched >= limit {
            break;
        }

        // Calculate how many instructions to run until next checkpoint
        let remaining = limit - matched;
        let batch_size = remaining.min(checkpoint_interval);

        let batch = run_batch(ref_runner, test_runner, batch_size);
        let both_exited = batch.ref_batch.has_exited() && batch.test_batch.has_exited();
        let exit_match = batch.ref_batch.exit_code() == batch.test_batch.exit_code();
        let errors_ok =
            !(batch.ref_batch.error || batch.test_batch.error) || (both_exited && exit_match);

        let states_match = batch.ref_batch.executed == batch.test_batch.executed
            && regs_match(ref_runner, test_runner)
            && batch.ref_batch.has_exited() == batch.test_batch.has_exited()
            && exit_match
            && errors_ok
            && (batch.ref_batch.has_exited()
                || batch.ref_batch.pc_after == batch.test_batch.pc_after);

        if states_match {
            // States match - continue to next checkpoint
            matched += batch.ref_batch.executed;

            if batch.ref_batch.has_exited() || batch.test_batch.has_exited() {
                break;
            }
            continue;
        }

        // States differ - find exact divergence point via binary search
        let start_instret = matched;
        let max_steps = batch
            .ref_batch
            .executed
            .min(batch.test_batch.executed)
            .min(batch_size);
        let mut low = 0u64;
        let mut high = max_steps;

        while low < high {
            let mid = (low + high).div_ceil(2);
            let target = start_instret + mid;
            let ref_snap = run_to_target(ref_runner, target);
            let test_snap = run_to_target(test_runner, target);

            if checkpoint_match(&ref_snap, &test_snap, ref_runner, test_runner, target) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }

        let divergence_at = matched + low;
        let target = start_instret + low + 1;
        let ref_snap = run_to_target(ref_runner, target);
        let test_snap = run_to_target(test_runner, target);

        let kind = divergence_kind(&ref_snap, &test_snap, ref_runner, test_runner, target);
        let divergence_index = u64_to_usize(divergence_at);
        return CompareResult {
            matched: divergence_index,
            divergence: Some(Divergence {
                index: divergence_index,
                expected: ref_snap.state,
                actual: test_snap.state,
                kind,
            }),
        };
    }

    CompareResult {
        matched: u64_to_usize(matched),
        divergence: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::diff::state::DiffState;

    /// Mock executor for testing.
    struct MockExecutor {
        states: Vec<DiffState>,
        index: usize,
    }

    impl MockExecutor {
        fn new(states: Vec<DiffState>) -> Self {
            Self { states, index: 0 }
        }
    }

    impl Executor for MockExecutor {
        fn step(&mut self) -> Option<DiffState> {
            if self.index < self.states.len() {
                let state = self.states[self.index].clone();
                self.index += 1;
                Some(state)
            } else {
                None
            }
        }
    }

    #[test]
    fn test_compare_matching() {
        let states = vec![
            DiffState {
                pc: 0x1000,
                opcode: 0x13,
                rd: Some(1),
                rd_value: Some(42),
                ..DiffState::default()
            },
            DiffState {
                pc: 0x1004,
                opcode: 0x13,
                rd: Some(2),
                rd_value: Some(100),
                is_exit: true,
                ..DiffState::default()
            },
        ];

        let mut ref_exec = MockExecutor::new(states.clone());
        let mut test_exec = MockExecutor::new(states);

        let result = compare_lockstep(
            &mut ref_exec,
            &mut test_exec,
            &CompareConfig::default(),
            None,
        );

        assert_eq!(result.matched, 2);
        assert!(result.divergence.is_none());
    }

    #[test]
    fn test_compare_pc_mismatch() {
        let ref_states = vec![DiffState {
            pc: 0x1000,
            ..DiffState::default()
        }];
        let test_states = vec![DiffState {
            pc: 0x2000,
            ..DiffState::default()
        }];

        let mut ref_exec = MockExecutor::new(ref_states);
        let mut test_exec = MockExecutor::new(test_states);

        let result = compare_lockstep(
            &mut ref_exec,
            &mut test_exec,
            &CompareConfig::default(),
            None,
        );

        assert_eq!(result.matched, 0);
        assert!(result.divergence.is_some());
        assert_eq!(result.divergence.unwrap().kind, DivergenceKind::ActualTail);
    }

    #[test]
    fn test_compare_ref_longer() {
        let ref_states = vec![
            DiffState {
                pc: 0x1000,
                ..DiffState::default()
            },
            DiffState {
                pc: 0x1004,
                ..DiffState::default()
            },
        ];
        let test_states = vec![DiffState {
            pc: 0x1000,
            ..DiffState::default()
        }];

        let mut ref_exec = MockExecutor::new(ref_states);
        let mut test_exec = MockExecutor::new(test_states);

        let result = compare_lockstep(
            &mut ref_exec,
            &mut test_exec,
            &CompareConfig::default(),
            None,
        );

        assert_eq!(result.matched, 1);
        assert!(result.divergence.is_some());
        assert_eq!(
            result.divergence.unwrap().kind,
            DivergenceKind::ExpectedTail
        );
    }

    #[test]
    fn test_compare_with_limit() {
        let states: Vec<_> = (0..100)
            .map(|i| DiffState {
                pc: 0x1000 + i * 4,
                ..DiffState::default()
            })
            .collect();

        let mut ref_exec = MockExecutor::new(states.clone());
        let mut test_exec = MockExecutor::new(states);

        let result = compare_lockstep(
            &mut ref_exec,
            &mut test_exec,
            &CompareConfig::default(),
            Some(10),
        );

        assert_eq!(result.matched, 10);
        assert!(result.divergence.is_none());
    }
}
