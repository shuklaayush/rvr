use super::{CompareConfig, DivergenceKind, TraceComparison, TraceDivergence, TraceEntry};

/// ECALL opcode (SYSTEM instruction with funct3=0, no registers).
const ECALL_OPCODE: u32 = 0x00000073;

/// EBREAK opcode.
const EBREAK_OPCODE: u32 = 0x00100073;

/// Check if an opcode is SC.W or SC.D.
fn is_sc(opcode: u32) -> bool {
    let op = opcode & 0x7f;
    let funct5 = (opcode >> 27) & 0x1f;
    op == 0x2f && funct5 == 0b00011
}

/// Check if a PC is likely in the trap handler region.
///
/// Uses the entry point to determine: trap handlers are typically placed
/// just before or at the entry point in riscv-tests.
fn is_trap_handler_pc(pc: u64, entry_point: u64) -> bool {
    let start = entry_point.saturating_sub(0x100);
    pc >= start && pc < entry_point + 0x50
}

/// Compare two traces sequentially, tolerating missing entries.
///
/// This handles cases where one trace has instructions the other doesn't log
/// (e.g., CSR writes that Spike doesn't report or rvr executes but doesn't log).
/// When PCs don't match, we try to skip entries to resync.
///
/// Special handling for ECALL/EBREAK at end of execution:
/// - rvr handles syscalls directly and traces the ECALL instruction
/// - Spike traps to machine mode and traces the trap handler instead
/// - When rvr ends with ECALL and Spike continues with trap handler, that's expected
pub fn compare_traces_with_config(
    expected: &[TraceEntry],
    actual: &[TraceEntry],
    config: &CompareConfig,
) -> TraceComparison {
    let mut exp_idx = 0;
    let mut act_idx = 0;
    let mut matched = 0;
    let mut first_divergence: Option<TraceDivergence> = None;

    while exp_idx < expected.len() && act_idx < actual.len() {
        let exp = &expected[exp_idx];
        let act = &actual[act_idx];

        if exp.pc == act.pc {
            // Same PC - compare the instruction
            if exp.opcode != act.opcode {
                let divergence = TraceDivergence {
                    index: matched,
                    expected: exp.clone(),
                    actual: act.clone(),
                    kind: DivergenceKind::Opcode,
                };
                if config.stop_on_first {
                    return TraceComparison {
                        matched,
                        divergence: Some(divergence),
                    };
                }
                if first_divergence.is_none() {
                    first_divergence = Some(divergence);
                }
                exp_idx += 1;
                act_idx += 1;
                continue;
            }

            // Check register write presence mismatch
            if config.strict_reg_writes {
                match (exp.rd.is_some(), act.rd.is_some()) {
                    (true, false) => {
                        let divergence = TraceDivergence {
                            index: matched,
                            expected: exp.clone(),
                            actual: act.clone(),
                            kind: DivergenceKind::MissingRegWrite,
                        };
                        if config.stop_on_first {
                            return TraceComparison {
                                matched,
                                divergence: Some(divergence),
                            };
                        }
                        if first_divergence.is_none() {
                            first_divergence = Some(divergence);
                        }
                        exp_idx += 1;
                        act_idx += 1;
                        continue;
                    }
                    (false, true) => {
                        let divergence = TraceDivergence {
                            index: matched,
                            expected: exp.clone(),
                            actual: act.clone(),
                            kind: DivergenceKind::ExtraRegWrite,
                        };
                        if config.stop_on_first {
                            return TraceComparison {
                                matched,
                                divergence: Some(divergence),
                            };
                        }
                        if first_divergence.is_none() {
                            first_divergence = Some(divergence);
                        }
                        exp_idx += 1;
                        act_idx += 1;
                        continue;
                    }
                    _ => {}
                }
            }

            // Check register write values (only if both have one)
            if exp.rd.is_some() && act.rd.is_some() {
                if exp.rd != act.rd {
                    let divergence = TraceDivergence {
                        index: matched,
                        expected: exp.clone(),
                        actual: act.clone(),
                        kind: DivergenceKind::RegDest,
                    };
                    if config.stop_on_first {
                        return TraceComparison {
                            matched,
                            divergence: Some(divergence),
                        };
                    }
                    if first_divergence.is_none() {
                        first_divergence = Some(divergence);
                    }
                    exp_idx += 1;
                    act_idx += 1;
                    continue;
                }

                if exp.rd_value != act.rd_value && !is_sc(exp.opcode) {
                    let divergence = TraceDivergence {
                        index: matched,
                        expected: exp.clone(),
                        actual: act.clone(),
                        kind: DivergenceKind::RegValue,
                    };
                    if config.stop_on_first {
                        return TraceComparison {
                            matched,
                            divergence: Some(divergence),
                        };
                    }
                    if first_divergence.is_none() {
                        first_divergence = Some(divergence);
                    }
                    exp_idx += 1;
                    act_idx += 1;
                    continue;
                }
            }

            // Check memory access presence mismatch
            if config.strict_mem_access {
                match (exp.mem_addr.is_some(), act.mem_addr.is_some()) {
                    (true, false) => {
                        if is_sc(exp.opcode) {
                            // SC may or may not perform the store.
                        } else {
                            let divergence = TraceDivergence {
                                index: matched,
                                expected: exp.clone(),
                                actual: act.clone(),
                                kind: DivergenceKind::MissingMemAccess,
                            };
                            if config.stop_on_first {
                                return TraceComparison {
                                    matched,
                                    divergence: Some(divergence),
                                };
                            }
                            if first_divergence.is_none() {
                                first_divergence = Some(divergence);
                            }
                            exp_idx += 1;
                            act_idx += 1;
                            continue;
                        }
                    }
                    (false, true) => {
                        if is_sc(exp.opcode) {
                            // SC may or may not perform the store.
                        } else {
                            let divergence = TraceDivergence {
                                index: matched,
                                expected: exp.clone(),
                                actual: act.clone(),
                                kind: DivergenceKind::ExtraMemAccess,
                            };
                            if config.stop_on_first {
                                return TraceComparison {
                                    matched,
                                    divergence: Some(divergence),
                                };
                            }
                            if first_divergence.is_none() {
                                first_divergence = Some(divergence);
                            }
                            exp_idx += 1;
                            act_idx += 1;
                            continue;
                        }
                    }
                    _ => {}
                }
            }

            // Check memory address (only if both have one)
            if exp.mem_addr.is_some()
                && act.mem_addr.is_some()
                && exp.mem_addr != act.mem_addr
                && !is_sc(exp.opcode)
            {
                let divergence = TraceDivergence {
                    index: matched,
                    expected: exp.clone(),
                    actual: act.clone(),
                    kind: DivergenceKind::MemAddr,
                };
                if config.stop_on_first {
                    return TraceComparison {
                        matched,
                        divergence: Some(divergence),
                    };
                }
                if first_divergence.is_none() {
                    first_divergence = Some(divergence);
                }
                exp_idx += 1;
                act_idx += 1;
                continue;
            }

            matched += 1;
            exp_idx += 1;
            act_idx += 1;
        } else {
            // PCs don't match - try to resync by scanning ahead.
            let window = 32usize;
            let mut skip_exp = None;
            let mut skip_act = None;

            // Prefer resync on (pc, opcode) to avoid false alignment on reused PCs.
            let mut skip_exp_pc = None;
            let mut skip_act_pc = None;
            for i in 1..=window {
                if exp_idx + i < expected.len() {
                    let cand = &expected[exp_idx + i];
                    if cand.pc == act.pc && cand.opcode == act.opcode {
                        skip_exp = Some(i);
                        break;
                    }
                    if skip_exp_pc.is_none() && cand.pc == act.pc {
                        skip_exp_pc = Some(i);
                    }
                }
            }
            for i in 1..=window {
                if act_idx + i < actual.len() {
                    let cand = &actual[act_idx + i];
                    if cand.pc == exp.pc && cand.opcode == exp.opcode {
                        skip_act = Some(i);
                        break;
                    }
                    if skip_act_pc.is_none() && cand.pc == exp.pc {
                        skip_act_pc = Some(i);
                    }
                }
            }
            if skip_exp.is_none() {
                skip_exp = skip_exp_pc;
            }
            if skip_act.is_none() {
                skip_act = skip_act_pc;
            }

            if let (Some(se), Some(sa)) = (skip_exp, skip_act) {
                if se <= sa {
                    exp_idx += se;
                } else {
                    act_idx += sa;
                }
            } else if let Some(se) = skip_exp {
                exp_idx += se;
            } else if let Some(sa) = skip_act {
                act_idx += sa;
            } else {
                // Can't resync - check for expected ECALL divergence
                // When rvr traces ECALL/EBREAK and Spike traces trap handler,
                // this is expected behavior (rvr handles syscalls directly)
                let is_ecall_divergence = (act.opcode == ECALL_OPCODE
                    || act.opcode == EBREAK_OPCODE)
                    && is_trap_handler_pc(exp.pc, config.entry_point);

                if is_ecall_divergence {
                    // rvr ends with ECALL, Spike continues in trap handler
                    // This is expected - treat as success
                    matched += 1; // Count the ECALL as matched
                    return TraceComparison {
                        matched,
                        divergence: None,
                    };
                }

                // Real control flow divergence
                let divergence = TraceDivergence {
                    index: matched,
                    expected: exp.clone(),
                    actual: act.clone(),
                    kind: DivergenceKind::Pc,
                };
                if config.stop_on_first {
                    return TraceComparison {
                        matched,
                        divergence: Some(divergence),
                    };
                }
                if first_divergence.is_none() {
                    first_divergence = Some(divergence);
                }
                exp_idx += 1;
                act_idx += 1;
                continue;
            }
        }
    }

    if let Some(divergence) = first_divergence {
        return TraceComparison {
            matched,
            divergence: Some(divergence),
        };
    }

    if exp_idx < expected.len() {
        let exp = expected[exp_idx].clone();
        let act = actual.get(act_idx).cloned().unwrap_or_else(|| exp.clone());
        return TraceComparison {
            matched,
            divergence: Some(TraceDivergence {
                index: matched,
                expected: exp,
                actual: act,
                kind: DivergenceKind::ExpectedTail,
            }),
        };
    }
    if act_idx < actual.len() {
        let act = actual[act_idx].clone();
        let exp = expected
            .get(exp_idx)
            .cloned()
            .unwrap_or_else(|| act.clone());
        return TraceComparison {
            matched,
            divergence: Some(TraceDivergence {
                index: matched,
                expected: exp,
                actual: act,
                kind: DivergenceKind::ActualTail,
            }),
        };
    }

    TraceComparison {
        matched,
        divergence: None,
    }
}

/// Align traces by finding first common PC at or after the entry point.
///
/// Spike has startup code at 0x1000 before jumping to the entry point.
/// rvr starts directly at the entry point.
pub fn align_traces_at(
    spike: &[TraceEntry],
    rvr: &[TraceEntry],
    entry_point: u64,
) -> (Vec<TraceEntry>, Vec<TraceEntry>) {
    // Find first instruction at entry_point or above in Spike trace
    let spike_start = spike.iter().position(|e| e.pc >= entry_point).unwrap_or(0);

    // Find first instruction at entry_point or above in rvr trace
    let rvr_start = rvr.iter().position(|e| e.pc >= entry_point).unwrap_or(0);

    (spike[spike_start..].to_vec(), rvr[rvr_start..].to_vec())
}
