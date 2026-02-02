//! Comparison algorithms for differential execution.

use super::executor::Executor;
use super::state::{compare_states, CompareConfig, CompareResult, Divergence, DiffGranularity, DivergenceKind};

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

        let ref_state = ref_exec.step();
        let test_state = test_exec.step();

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

/// Comparison runner that handles all granularity modes.
pub struct DiffRunner<'a> {
    ref_exec: &'a mut dyn Executor,
    test_exec: &'a mut dyn Executor,
    config: CompareConfig,
    granularity: DiffGranularity,
    max_instrs: Option<u64>,
}

impl<'a> DiffRunner<'a> {
    /// Create a new diff runner.
    pub fn new(
        ref_exec: &'a mut dyn Executor,
        test_exec: &'a mut dyn Executor,
        config: CompareConfig,
        granularity: DiffGranularity,
        max_instrs: Option<u64>,
    ) -> Self {
        Self {
            ref_exec,
            test_exec,
            config,
            granularity,
            max_instrs,
        }
    }

    /// Run the comparison.
    pub fn run(self) -> CompareResult {
        match self.granularity {
            DiffGranularity::Instruction => {
                compare_lockstep(self.ref_exec, self.test_exec, &self.config, self.max_instrs)
            }
            DiffGranularity::Block | DiffGranularity::Hybrid => {
                // For now, block and hybrid fall back to instruction mode
                // Block mode would step by CFG block boundaries
                // Hybrid would do block mode and drill down on divergence
                compare_lockstep(self.ref_exec, self.test_exec, &self.config, self.max_instrs)
            }
        }
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

        fn current_pc(&self) -> u64 {
            self.states.get(self.index).map(|s| s.pc).unwrap_or(0)
        }

        fn instret(&self) -> u64 {
            self.index as u64
        }

        fn has_exited(&self) -> bool {
            self.index >= self.states.len()
        }

        fn exit_code(&self) -> Option<u8> {
            if self.has_exited() {
                Some(0)
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
        assert_eq!(result.divergence.unwrap().kind, DivergenceKind::ExpectedTail);
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
