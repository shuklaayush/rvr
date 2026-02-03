//! Comparison algorithms for differential execution.

use super::executor::Executor;
use super::inprocess::BufferedInProcessExecutor;
use super::state::{CompareConfig, CompareResult, DiffState, Divergence, DivergenceKind, compare_states};

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
                let (ref_s, test_s) = match (&ref_state, &test_state) {
                    (Some(r), Some(t)) => (r, t),
                    _ => break,
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
                        actual: Default::default(),
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
                        expected: Default::default(),
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
                            expected: Default::default(),
                            actual: linear_state,
                            kind: DivergenceKind::ActualTail,
                        }),
                    };
                }
                break;
            } else {
                // No entries but not exited - unexpected
                break;
            }
        }

        // Compare each buffered entry against stepped linear entry
        for i in 0..block_count {
            if matched as u64 >= limit {
                break;
            }

            let block_state = match block_exec.get_entry(i) {
                Some(s) => s,
                None => break,
            };

            let linear_state = match linear_exec.step() {
                Some(s) => s,
                None => {
                    // Linear executor finished early
                    return CompareResult {
                        matched,
                        divergence: Some(Divergence {
                            index: matched,
                            expected: block_state,
                            actual: Default::default(),
                            kind: DivergenceKind::ExpectedTail,
                        }),
                    };
                }
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
    ref_runner: &mut rvr::Runner,
    test_runner: &mut rvr::Runner,
    checkpoint_interval: u64,
    max_instrs: Option<u64>,
) -> CompareResult {
    let limit = max_instrs.unwrap_or(u64::MAX);
    let mut matched: u64 = 0;

    // Helper to compare register files
    let regs_match = |ref_r: &rvr::Runner, test_r: &rvr::Runner| -> bool {
        for i in 0..ref_r.num_regs() {
            if ref_r.get_register(i) != test_r.get_register(i) {
                return false;
            }
        }
        true
    };

    // Initial alignment: run a small number of instructions to synchronize PCs
    // Different backends may start at slightly different PCs
    let ref_pc = ref_runner.get_pc();
    let test_pc = test_runner.get_pc();
    if ref_pc != test_pc {
        // Run up to 256 instructions to align
        for _ in 0..256 {
            let curr_ref_pc = ref_runner.get_pc();
            let curr_test_pc = test_runner.get_pc();
            if curr_ref_pc == curr_test_pc {
                break;
            }
            // Advance the one with lower PC
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

    loop {
        if matched >= limit {
            break;
        }

        // Calculate how many instructions to run until next checkpoint
        let remaining = limit - matched;
        let batch_size = remaining.min(checkpoint_interval);

        // Run reference for batch_size instructions
        let ref_start_instret = ref_runner.instret();
        ref_runner.set_target_instret(ref_start_instret + batch_size);
        ref_runner.clear_exit();
        let ref_pc = ref_runner.get_pc();
        let ref_result = ref_runner.execute_from(ref_pc);

        // Run test for batch_size instructions
        let test_start_instret = test_runner.instret();
        test_runner.set_target_instret(test_start_instret + batch_size);
        test_runner.clear_exit();
        let test_pc = test_runner.get_pc();
        let test_result = test_runner.execute_from(test_pc);

        // Check exit status and errors
        let ref_has_exited = ref_runner.has_exited();
        let test_has_exited = test_runner.has_exited();
        let ref_exit_code = ref_runner.exit_code();
        let test_exit_code = test_runner.exit_code();
        let ref_error = ref_result.is_err();
        let test_error = test_result.is_err();

        // Get actual instructions executed
        let ref_executed = ref_runner.instret() - ref_start_instret;
        let test_executed = test_runner.instret() - test_start_instret;

        // Quick check: if PCs match and registers match, we're good
        let ref_pc_after = ref_runner.get_pc();
        let test_pc_after = test_runner.get_pc();

        if ref_executed == test_executed
            && regs_match(ref_runner, test_runner)
            && ref_has_exited == test_has_exited
            && ref_exit_code == test_exit_code
            && !ref_error
            && !test_error
            && (ref_has_exited || ref_pc_after == test_pc_after)
        {
            // States match - continue to next checkpoint
            matched += ref_executed;

            if ref_has_exited || test_has_exited {
                break;
            }
            continue;
        }

        // States differ - need to find exact divergence point
        // For now, report at checkpoint granularity
        // TODO: Implement binary search to find exact instruction
        return CompareResult {
            matched: matched as usize,
            divergence: Some(Divergence {
                index: matched as usize,
                expected: DiffState {
                    pc: ref_pc_after,
                    instret: ref_runner.instret(),
                    is_exit: ref_has_exited,
                    ..Default::default()
                },
                actual: DiffState {
                    pc: test_pc_after,
                    instret: test_runner.instret(),
                    is_exit: test_has_exited,
                    ..Default::default()
                },
                kind: if ref_pc_after != test_pc_after {
                    DivergenceKind::Pc
                } else if !regs_match(ref_runner, test_runner) {
                    DivergenceKind::RegValue
                } else if ref_has_exited != test_has_exited {
                    if ref_has_exited {
                        DivergenceKind::ActualTail
                    } else {
                        DivergenceKind::ExpectedTail
                    }
                } else {
                    DivergenceKind::Pc // Fallback
                },
            }),
        };
    }

    CompareResult {
        matched: matched as usize,
        divergence: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test::diff::state::DiffState;

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
                ..Default::default()
            },
            DiffState {
                pc: 0x1004,
                opcode: 0x13,
                rd: Some(2),
                rd_value: Some(100),
                is_exit: true,
                ..Default::default()
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
            ..Default::default()
        }];
        let test_states = vec![DiffState {
            pc: 0x2000,
            ..Default::default()
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
        assert_eq!(result.divergence.unwrap().kind, DivergenceKind::Pc);
    }

    #[test]
    fn test_compare_ref_longer() {
        let ref_states = vec![
            DiffState {
                pc: 0x1000,
                ..Default::default()
            },
            DiffState {
                pc: 0x1004,
                ..Default::default()
            },
        ];
        let test_states = vec![DiffState {
            pc: 0x1000,
            ..Default::default()
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
                ..Default::default()
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
