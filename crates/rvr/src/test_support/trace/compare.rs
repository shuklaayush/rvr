use super::{CompareConfig, DivergenceKind, TraceComparison, TraceDivergence, TraceEntry};

/// ECALL opcode (SYSTEM instruction with funct3=0, no registers).
const ECALL_OPCODE: u32 = 0x0000_0073;

/// EBREAK opcode.
const EBREAK_OPCODE: u32 = 0x0010_0073;

/// Check if an opcode is SC.W or SC.D.
const fn is_sc(opcode: u32) -> bool {
    let op = opcode & 0x7f;
    let funct5 = (opcode >> 27) & 0x1f;
    op == 0x2f && funct5 == 0b00011
}

/// Check if a PC is likely in the trap handler region.
///
/// Uses the entry point to determine: trap handlers are typically placed
/// just before or at the entry point in riscv-tests.
const fn is_trap_handler_pc(pc: u64, entry_point: u64) -> bool {
    let start = entry_point.saturating_sub(0x100);
    pc >= start && pc < entry_point + 0x50
}

enum CompareStep {
    AdvanceMatched,
    AdvanceUnmatched,
    Return(TraceComparison),
}

enum ResyncAction {
    SkipExpected(usize),
    SkipActual(usize),
}

fn record_divergence(
    config: &CompareConfig,
    matched: usize,
    expected: &TraceEntry,
    actual: &TraceEntry,
    kind: DivergenceKind,
    first_divergence: &mut Option<TraceDivergence>,
) -> Option<TraceComparison> {
    let divergence = TraceDivergence {
        index: matched,
        expected: expected.clone(),
        actual: actual.clone(),
        kind,
    };
    if config.stop_on_first {
        return Some(TraceComparison {
            matched,
            divergence: Some(divergence),
        });
    }
    if first_divergence.is_none() {
        *first_divergence = Some(divergence);
    }
    None
}

fn compare_same_pc(
    expected: &TraceEntry,
    actual: &TraceEntry,
    matched: usize,
    config: &CompareConfig,
    first_divergence: &mut Option<TraceDivergence>,
) -> CompareStep {
    if let Some(step) = check_opcode(expected, actual, matched, config, first_divergence) {
        return step;
    }
    if let Some(step) = check_reg_writes(expected, actual, matched, config, first_divergence) {
        return step;
    }
    if let Some(step) = check_reg_dest_value(expected, actual, matched, config, first_divergence) {
        return step;
    }
    if let Some(step) = check_mem_access(expected, actual, matched, config, first_divergence) {
        return step;
    }
    if let Some(step) = check_mem_addr(expected, actual, matched, config, first_divergence) {
        return step;
    }
    CompareStep::AdvanceMatched
}

fn divergence_step(
    config: &CompareConfig,
    matched: usize,
    expected: &TraceEntry,
    actual: &TraceEntry,
    kind: DivergenceKind,
    first_divergence: &mut Option<TraceDivergence>,
) -> CompareStep {
    record_divergence(config, matched, expected, actual, kind, first_divergence)
        .map_or(CompareStep::AdvanceUnmatched, CompareStep::Return)
}

fn check_opcode(
    expected: &TraceEntry,
    actual: &TraceEntry,
    matched: usize,
    config: &CompareConfig,
    first_divergence: &mut Option<TraceDivergence>,
) -> Option<CompareStep> {
    if expected.opcode == actual.opcode {
        return None;
    }
    Some(divergence_step(
        config,
        matched,
        expected,
        actual,
        DivergenceKind::Opcode,
        first_divergence,
    ))
}

fn check_reg_writes(
    expected: &TraceEntry,
    actual: &TraceEntry,
    matched: usize,
    config: &CompareConfig,
    first_divergence: &mut Option<TraceDivergence>,
) -> Option<CompareStep> {
    if !config.strict_reg_writes {
        return None;
    }
    match (expected.rd.is_some(), actual.rd.is_some()) {
        (true, false) => Some(divergence_step(
            config,
            matched,
            expected,
            actual,
            DivergenceKind::MissingRegWrite,
            first_divergence,
        )),
        (false, true) => Some(divergence_step(
            config,
            matched,
            expected,
            actual,
            DivergenceKind::ExtraRegWrite,
            first_divergence,
        )),
        _ => None,
    }
}

fn check_reg_dest_value(
    expected: &TraceEntry,
    actual: &TraceEntry,
    matched: usize,
    config: &CompareConfig,
    first_divergence: &mut Option<TraceDivergence>,
) -> Option<CompareStep> {
    if expected.rd.is_none() || actual.rd.is_none() {
        return None;
    }
    if expected.rd != actual.rd {
        return Some(divergence_step(
            config,
            matched,
            expected,
            actual,
            DivergenceKind::RegDest,
            first_divergence,
        ));
    }
    if expected.rd_value != actual.rd_value && !is_sc(expected.opcode) {
        return Some(divergence_step(
            config,
            matched,
            expected,
            actual,
            DivergenceKind::RegValue,
            first_divergence,
        ));
    }
    None
}

fn check_mem_access(
    expected: &TraceEntry,
    actual: &TraceEntry,
    matched: usize,
    config: &CompareConfig,
    first_divergence: &mut Option<TraceDivergence>,
) -> Option<CompareStep> {
    if !config.strict_mem_access {
        return None;
    }
    match (expected.mem_addr.is_some(), actual.mem_addr.is_some()) {
        (true, false) if !is_sc(expected.opcode) => Some(divergence_step(
            config,
            matched,
            expected,
            actual,
            DivergenceKind::MissingMemAccess,
            first_divergence,
        )),
        (false, true) if !is_sc(expected.opcode) => Some(divergence_step(
            config,
            matched,
            expected,
            actual,
            DivergenceKind::ExtraMemAccess,
            first_divergence,
        )),
        _ => None,
    }
}

fn check_mem_addr(
    expected: &TraceEntry,
    actual: &TraceEntry,
    matched: usize,
    config: &CompareConfig,
    first_divergence: &mut Option<TraceDivergence>,
) -> Option<CompareStep> {
    if expected.mem_addr.is_some()
        && actual.mem_addr.is_some()
        && expected.mem_addr != actual.mem_addr
        && !is_sc(expected.opcode)
    {
        return Some(divergence_step(
            config,
            matched,
            expected,
            actual,
            DivergenceKind::MemAddr,
            first_divergence,
        ));
    }
    None
}

fn find_resync_action(
    expected: &[TraceEntry],
    actual: &[TraceEntry],
    exp_idx: usize,
    act_idx: usize,
    window: usize,
) -> Option<ResyncAction> {
    let exp = &expected[exp_idx];
    let act = &actual[act_idx];
    let mut skip_exp = None;
    let mut skip_act = None;
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

    match (skip_exp, skip_act) {
        (Some(se), Some(sa)) => {
            if se <= sa {
                Some(ResyncAction::SkipExpected(se))
            } else {
                Some(ResyncAction::SkipActual(sa))
            }
        }
        (Some(se), None) => Some(ResyncAction::SkipExpected(se)),
        (None, Some(sa)) => Some(ResyncAction::SkipActual(sa)),
        (None, None) => None,
    }
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
#[must_use]
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
            match compare_same_pc(exp, act, matched, config, &mut first_divergence) {
                CompareStep::Return(result) => return result,
                CompareStep::AdvanceMatched => {
                    matched += 1;
                    exp_idx += 1;
                    act_idx += 1;
                }
                CompareStep::AdvanceUnmatched => {
                    exp_idx += 1;
                    act_idx += 1;
                }
            }
            continue;
        }

        if let Some(action) = find_resync_action(expected, actual, exp_idx, act_idx, 32) {
            match action {
                ResyncAction::SkipExpected(se) => exp_idx += se,
                ResyncAction::SkipActual(sa) => act_idx += sa,
            }
            continue;
        }

        let is_ecall_divergence = (act.opcode == ECALL_OPCODE || act.opcode == EBREAK_OPCODE)
            && is_trap_handler_pc(exp.pc, config.entry_point);
        if is_ecall_divergence {
            matched += 1;
            return TraceComparison {
                matched,
                divergence: None,
            };
        }

        if let Some(result) = record_divergence(
            config,
            matched,
            exp,
            act,
            DivergenceKind::Pc,
            &mut first_divergence,
        ) {
            return result;
        }
        exp_idx += 1;
        act_idx += 1;
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
#[must_use]
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
