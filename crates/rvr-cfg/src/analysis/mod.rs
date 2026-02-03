//! Control flow analysis used to identify basic block leaders and targets.

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace, trace_span};

use rvr_isa::{InstrArgs, Xlen};

use crate::InstructionTable;

const NUM_REGS: usize = 32;
const MAX_VALUES: usize = 16;
const MAX_ITERATIONS_MULTIPLIER: usize = 20;
const MAX_JUMP_TABLE_SCAN: usize = 256;

mod data;

use data::{DecodedInstruction, InstrKind, RegisterState, RegisterValue};

pub struct ControlFlowResult {
    pub successors: FxHashMap<u64, FxHashSet<u64>>,
    pub predecessors: FxHashMap<u64, FxHashSet<u64>>,
    pub unresolved_dynamic_jumps: FxHashSet<u64>,
    pub leaders: FxHashSet<u64>,
    pub call_return_map: FxHashMap<u64, FxHashSet<u64>>,
    pub block_to_function: FxHashMap<u64, u64>,
}

pub struct ControlFlowAnalyzer;

impl ControlFlowAnalyzer {
    pub fn analyze<X: Xlen>(instruction_table: &InstructionTable<X>) -> ControlFlowResult {
        let (function_entries, internal_targets, return_sites) = {
            let _span = trace_span!("collect_targets").entered();
            collect_potential_targets(instruction_table)
        };

        let call_return_map = {
            let _span = trace_span!("build_call_return_map").entered();
            build_call_return_map(instruction_table)
        };

        let mut sorted_function_entries: Vec<u64> = function_entries.iter().copied().collect();
        sorted_function_entries.sort_unstable();

        let mut func_internal_targets: FxHashMap<u64, FxHashSet<u64>> = FxHashMap::default();
        for target in &internal_targets {
            if let Some(func_start) = binary_search_le(&sorted_function_entries, *target) {
                func_internal_targets
                    .entry(func_start)
                    .or_default()
                    .insert(*target);
            }
        }

        let (successors, unresolved_dynamic_jumps) = {
            let _span = trace_span!("worklist").entered();
            worklist(
                instruction_table,
                &function_entries,
                &internal_targets,
                &return_sites,
                &sorted_function_entries,
                &func_internal_targets,
                &call_return_map,
            )
        };

        let leaders = {
            let _span = trace_span!("compute_leaders").entered();
            compute_leaders(
                instruction_table,
                &successors,
                &function_entries,
                &internal_targets,
                &return_sites,
            )
        };

        debug!(
            functions = function_entries.len(),
            leaders = leaders.len(),
            unresolved = unresolved_dynamic_jumps.len(),
            "CFG analysis complete"
        );

        let mut block_to_function = FxHashMap::default();
        for leader in &leaders {
            if let Some(func) = binary_search_le(&sorted_function_entries, *leader) {
                block_to_function.insert(*leader, func);
            }
        }

        let predecessors = {
            let _span = trace_span!("build_predecessors").entered();
            build_predecessors(&successors)
        };

        ControlFlowResult {
            successors,
            predecessors,
            unresolved_dynamic_jumps,
            leaders,
            call_return_map,
            block_to_function,
        }
    }
}

fn collect_potential_targets<X: Xlen>(
    instruction_table: &InstructionTable<X>,
) -> (FxHashSet<u64>, FxHashSet<u64>, FxHashSet<u64>) {
    let mut function_entries = FxHashSet::default();
    let mut internal_targets = FxHashSet::default();
    let mut return_sites = FxHashSet::default();

    // Add all entry points (ELF entry + any library exports)
    function_entries.extend(instruction_table.entry_points().iter().copied());

    scan_ro_segments_for_code_pointers(instruction_table, &mut internal_targets);

    let mut regs: [Option<u64>; NUM_REGS] = [None; NUM_REGS];
    regs[0] = Some(0);

    let mut pc = instruction_table.base_address();
    let end = instruction_table.end_address();
    while pc < end {
        if !instruction_table.is_valid_pc(pc) {
            pc += InstructionTable::<X>::SLOT_SIZE as u64;
            continue;
        }

        let size = instruction_table.instruction_size_at_pc(pc) as u64;
        if size == 0 {
            pc += InstructionTable::<X>::SLOT_SIZE as u64;
            continue;
        }

        let instr = match instruction_table.get_at_pc(pc) {
            Some(instr) => instr,
            None => {
                pc += size;
                continue;
            }
        };

        let decoded = DecodedInstruction::from_instr(instr);
        match decoded.kind {
            InstrKind::Lui => {
                if let Some(rd) = decoded.rd {
                    regs[rd as usize] = Some(sign_extend_i32(decoded.imm));
                }
            }
            InstrKind::Auipc => {
                if let Some(rd) = decoded.rd {
                    regs[rd as usize] = Some(add_signed(pc, decoded.imm));
                }
            }
            InstrKind::Addi => {
                if let (Some(rd), Some(rs1)) = (decoded.rd, decoded.rs1) {
                    if let Some(base) = regs[rs1 as usize] {
                        let computed = add_signed(base, decoded.imm);
                        regs[rd as usize] = Some(computed);
                        if instruction_table.is_valid_pc(computed) {
                            function_entries.insert(computed);
                        }
                    } else {
                        regs[rd as usize] = None;
                    }
                }
            }
            InstrKind::Add => {
                if let (Some(rd), Some(rs1), Some(rs2)) = (decoded.rd, decoded.rs1, decoded.rs2) {
                    if let (Some(lhs), Some(rhs)) = (regs[rs1 as usize], regs[rs2 as usize]) {
                        let computed = lhs.wrapping_add(rhs);
                        regs[rd as usize] = Some(computed);
                        if instruction_table.is_valid_pc(computed) {
                            function_entries.insert(computed);
                        }
                    } else {
                        regs[rd as usize] = None;
                    }
                }
            }
            InstrKind::Move => {
                if let (Some(rd), Some(rs1)) = (decoded.rd, decoded.rs1) {
                    regs[rd as usize] = regs[rs1 as usize];
                }
            }
            InstrKind::Load => {
                if let (Some(rd), Some(rs1)) = (decoded.rd, decoded.rs1) {
                    if let Some(base) = regs[rs1 as usize] {
                        let addr = add_signed(base, decoded.imm);
                        let maybe_val =
                            instruction_table.read_readonly(addr, decoded.width as usize);
                        if let Some(raw) = maybe_val {
                            let extended =
                                extend_loaded_value(raw, decoded.width, decoded.is_unsigned);
                            regs[rd as usize] = Some(extended);
                            if instruction_table.is_valid_pc(extended) {
                                internal_targets.insert(extended);
                            }
                        } else {
                            regs[rd as usize] = None;
                        }
                    } else {
                        regs[rd as usize] = None;
                    }
                }
            }
            InstrKind::Jal => {
                let target = add_signed(pc, decoded.imm);
                if decoded.is_call() {
                    if instruction_table.is_valid_pc(target) {
                        function_entries.insert(target);
                        return_sites.insert(pc + size);
                    }
                } else if instruction_table.is_valid_pc(target) {
                    internal_targets.insert(target);
                }

                if let Some(rd) = decoded.rd {
                    regs[rd as usize] = Some(pc + size);
                }
            }
            InstrKind::Jalr => {
                if let Some(rs1) = decoded.rs1
                    && let Some(base) = regs[rs1 as usize]
                {
                    let target = add_signed(base, decoded.imm) & !1u64;
                    if instruction_table.is_valid_pc(target) {
                        function_entries.insert(target);
                    }
                }
                if decoded.is_call() {
                    return_sites.insert(pc + size);
                }
                if let Some(rd) = decoded.rd {
                    regs[rd as usize] = Some(pc + size);
                }
            }
            InstrKind::Branch => {
                let target = add_signed(pc, decoded.imm);
                if instruction_table.is_valid_pc(target) {
                    internal_targets.insert(target);
                }
                internal_targets.insert(pc + size);
            }
            InstrKind::Unknown => {
                if let Some(rd) = extract_written_reg(&instr.args) {
                    regs[rd as usize] = None;
                }
            }
        }

        pc += size;
    }

    (function_entries, internal_targets, return_sites)
}

fn build_call_return_map<X: Xlen>(
    instruction_table: &InstructionTable<X>,
) -> FxHashMap<u64, FxHashSet<u64>> {
    let mut call_return_map: FxHashMap<u64, FxHashSet<u64>> = FxHashMap::default();
    let mut pc = instruction_table.base_address();
    let end = instruction_table.end_address();

    while pc < end {
        if !instruction_table.is_valid_pc(pc) {
            pc += InstructionTable::<X>::SLOT_SIZE as u64;
            continue;
        }

        let size = instruction_table.instruction_size_at_pc(pc) as u64;
        if size == 0 {
            pc += InstructionTable::<X>::SLOT_SIZE as u64;
            continue;
        }

        let instr = match instruction_table.get_at_pc(pc) {
            Some(instr) => instr,
            None => {
                pc += size;
                continue;
            }
        };
        let decoded = DecodedInstruction::from_instr(instr);

        if decoded.is_static_call() {
            let callee = add_signed(pc, decoded.imm);
            if instruction_table.is_valid_pc(callee) {
                call_return_map.entry(callee).or_default().insert(pc + size);
            }
        }

        pc += size;
    }

    call_return_map
}

fn worklist<X: Xlen>(
    instruction_table: &InstructionTable<X>,
    function_entries: &FxHashSet<u64>,
    internal_targets: &FxHashSet<u64>,
    return_sites: &FxHashSet<u64>,
    sorted_function_entries: &[u64],
    func_internal_targets: &FxHashMap<u64, FxHashSet<u64>>,
    call_return_map: &FxHashMap<u64, FxHashSet<u64>>,
) -> (FxHashMap<u64, FxHashSet<u64>>, FxHashSet<u64>) {
    // Pre-allocate with estimated capacity to reduce rehashing
    let estimated_size = function_entries.len() + internal_targets.len();
    let mut states: FxHashMap<u64, RegisterState> =
        FxHashMap::with_capacity_and_hasher(estimated_size, Default::default());
    let mut worklist = Vec::with_capacity(estimated_size);
    let mut in_worklist: FxHashSet<u64> =
        FxHashSet::with_capacity_and_hasher(estimated_size, Default::default());
    let mut successors: FxHashMap<u64, FxHashSet<u64>> =
        FxHashMap::with_capacity_and_hasher(estimated_size, Default::default());
    let mut unresolved_dynamic_jumps: FxHashSet<u64> = FxHashSet::default();

    // Add all entry points to worklist
    for addr in function_entries {
        if in_worklist.insert(*addr) {
            states.insert(*addr, RegisterState::new());
            worklist.push(*addr);
        }
    }

    for addr in internal_targets {
        if in_worklist.insert(*addr) {
            states.insert(*addr, RegisterState::new());
            worklist.push(*addr);
        }
    }

    let max_iterations = (instruction_table.end_address() - instruction_table.base_address())
        as usize
        * MAX_ITERATIONS_MULTIPLIER;

    let mut idx = 0;
    while idx < worklist.len() {
        if idx > max_iterations {
            break;
        }

        let pc = worklist[idx];
        idx += 1;
        in_worklist.remove(&pc);

        let state = match states.get(&pc) {
            Some(state) => state.clone(),
            None => continue,
        };

        let size = instruction_table.instruction_size_at_pc(pc) as u64;
        if size == 0 {
            continue;
        }

        let instr = match instruction_table.get_at_pc(pc) {
            Some(instr) => instr,
            None => continue,
        };
        let decoded = DecodedInstruction::from_instr(instr);

        let succs = get_successors(
            instruction_table,
            pc,
            size,
            &decoded,
            &state,
            function_entries,
            return_sites,
            sorted_function_entries,
            func_internal_targets,
            call_return_map,
            &mut unresolved_dynamic_jumps,
        );

        let state_out = transfer(instruction_table, pc, size, &decoded, state);

        for target in &succs {
            if let Some(existing) = states.get_mut(target) {
                if existing.merge(&state_out) && in_worklist.insert(*target) {
                    worklist.push(*target);
                }
            } else {
                states.insert(*target, state_out.clone());
                if in_worklist.insert(*target) {
                    worklist.push(*target);
                }
            }
        }

        successors.entry(pc).or_default().extend(succs);
    }

    trace!(iterations = idx, "worklist complete");

    (successors, unresolved_dynamic_jumps)
}

// Many parameters needed for inter-procedural analysis context
#[allow(clippy::too_many_arguments)]
/// Scan forward from an indirect jump to find jump table targets.
///
/// This handles Duff's device patterns where a computed jump lands at various
/// points within a sequential instruction sequence. For example, optimized
/// memset/memcpy implementations compute an offset and jump into the middle
/// of a series of store instructions.
///
/// We scan forward collecting all instruction addresses until we hit a terminator
/// (ret, unconditional jump, or another indirect jump).
fn scan_jump_table_targets<X: Xlen>(
    instruction_table: &InstructionTable<X>,
    start_pc: u64,
) -> FxHashSet<u64> {
    let mut targets = FxHashSet::default();
    let mut pc = start_pc;
    let end = instruction_table.end_address();

    let mut count = 0;

    while pc < end && count < MAX_JUMP_TABLE_SCAN {
        if !instruction_table.is_valid_pc(pc) {
            break;
        }

        let size = instruction_table.instruction_size_at_pc(pc) as u64;
        if size == 0 {
            break;
        }

        let instr = match instruction_table.get_at_pc(pc) {
            Some(instr) => instr,
            None => break,
        };

        // This instruction is a valid jump target
        targets.insert(pc);

        let decoded = DecodedInstruction::from_instr(instr);

        // Stop at terminators
        if decoded.is_return() {
            break;
        }
        if decoded.kind == InstrKind::Jal && decoded.rd == Some(0) {
            // Unconditional jump (j instruction) - include target and stop
            let target = add_signed(pc, decoded.imm);
            if instruction_table.is_valid_pc(target) {
                targets.insert(target);
            }
            break;
        }
        if decoded.is_indirect_jump() {
            // Another computed jump - stop here
            break;
        }

        pc += size;
        count += 1;
    }

    targets
}

#[allow(clippy::too_many_arguments)]
fn get_successors<X: Xlen>(
    instruction_table: &InstructionTable<X>,
    pc: u64,
    size: u64,
    decoded: &DecodedInstruction,
    state: &RegisterState,
    function_entries: &FxHashSet<u64>,
    return_sites: &FxHashSet<u64>,
    sorted_function_entries: &[u64],
    func_internal_targets: &FxHashMap<u64, FxHashSet<u64>>,
    call_return_map: &FxHashMap<u64, FxHashSet<u64>>,
    unresolved_dynamic_jumps: &mut FxHashSet<u64>,
) -> FxHashSet<u64> {
    let mut result = FxHashSet::default();

    match decoded.kind {
        InstrKind::Jal => {
            let target = add_signed(pc, decoded.imm);
            if instruction_table.is_valid_pc(target) {
                result.insert(target);
            }
            if decoded.is_call() {
                result.insert(pc + size);
            }
        }
        InstrKind::Jalr => {
            let mut resolved = false;
            if let Some(rs1) = decoded.rs1 {
                let base = state.get_ref(rs1);
                if base.is_constant() && !base.values.is_empty() {
                    for value in &base.values {
                        let target = add_signed(*value, decoded.imm) & !1u64;
                        if instruction_table.is_valid_pc(target) {
                            result.insert(target);
                        }
                    }
                    resolved = true;
                    if decoded.is_call() {
                        result.insert(pc + size);
                    }
                }
            }

            if !resolved {
                if decoded.is_return() {
                    if let Some(func_start) = binary_search_le(sorted_function_entries, pc) {
                        if let Some(returns) = call_return_map.get(&func_start) {
                            result.extend(returns.iter().copied());
                        } else {
                            result.extend(return_sites.iter().copied());
                        }
                    } else {
                        result.extend(return_sites.iter().copied());
                    }
                } else if decoded.is_call() {
                    result.extend(function_entries.iter().copied());
                    result.insert(pc + size);
                } else if decoded.is_indirect_jump() {
                    // For indirect jumps (switch tables, tail calls), use pre-computed targets.
                    // These come from two sources:
                    // 1. scan_ro_segments_for_code_pointers: finds switch table entries in rodata
                    // 2. scan_jump_table_targets: finds Duff's device patterns (sequential targets)
                    //
                    // First try Duff's device pattern (sequential targets after this jump)
                    let duff_targets = scan_jump_table_targets(instruction_table, pc + size);
                    if !duff_targets.is_empty() {
                        result.extend(duff_targets);
                    }

                    // Also use internal targets from rodata scanning (switch tables)
                    if let Some(func_start) = binary_search_le(sorted_function_entries, pc)
                        && let Some(targets) = func_internal_targets.get(&func_start)
                    {
                        result.extend(targets.iter().copied());
                    }

                    // Fall back to function entries for potential tail calls
                    if result.is_empty() {
                        unresolved_dynamic_jumps.insert(pc);
                        result.extend(function_entries.iter().copied());
                    }
                }
            }
        }
        InstrKind::Branch => {
            result.insert(pc + size);
            let target = add_signed(pc, decoded.imm);
            if instruction_table.is_valid_pc(target) {
                result.insert(target);
            }
        }
        _ => {
            result.insert(pc + size);
        }
    }

    result
}

fn transfer<X: Xlen>(
    instruction_table: &InstructionTable<X>,
    pc: u64,
    size: u64,
    decoded: &DecodedInstruction,
    mut state: RegisterState,
) -> RegisterState {
    match decoded.kind {
        InstrKind::Lui => {
            if let Some(rd) = decoded.rd {
                state.set_constant(rd, sign_extend_i32(decoded.imm));
            }
        }
        InstrKind::Auipc => {
            if let Some(rd) = decoded.rd {
                state.set_constant(rd, add_signed(pc, decoded.imm));
            }
        }
        InstrKind::Addi => {
            if let (Some(rd), Some(rs1)) = (decoded.rd, decoded.rs1) {
                let base = state.get(rs1);
                if base.is_constant() && !base.values.is_empty() {
                    let mut result =
                        RegisterValue::constant(add_signed(base.values[0], decoded.imm));
                    for value in base.values.iter().skip(1) {
                        result.add_value(add_signed(*value, decoded.imm));
                        if !result.is_constant() {
                            break;
                        }
                    }
                    state.set(rd, result);
                } else {
                    state.set_unknown(rd);
                }
            }
        }
        InstrKind::Add => {
            if let (Some(rd), Some(rs1), Some(rs2)) = (decoded.rd, decoded.rs1, decoded.rs2) {
                let lhs = state.get(rs1);
                let rhs = state.get(rs2);
                if lhs.is_constant()
                    && rhs.is_constant()
                    && !lhs.values.is_empty()
                    && !rhs.values.is_empty()
                {
                    let mut result =
                        RegisterValue::constant(lhs.values[0].wrapping_add(rhs.values[0]));
                    'outer: for l in &lhs.values {
                        for r in &rhs.values {
                            if l == &lhs.values[0] && r == &rhs.values[0] {
                                continue;
                            }
                            result.add_value(l.wrapping_add(*r));
                            if !result.is_constant() {
                                break 'outer;
                            }
                        }
                    }
                    state.set(rd, result);
                } else {
                    state.set_unknown(rd);
                }
            }
        }
        InstrKind::Move => {
            if let (Some(rd), Some(rs1)) = (decoded.rd, decoded.rs1) {
                let value = state.get(rs1);
                state.set(rd, value);
            }
        }
        InstrKind::Load => {
            if let (Some(rd), Some(rs1)) = (decoded.rd, decoded.rs1) {
                let base = state.get(rs1);
                let mut resolved = false;
                if rs1 != 2 && base.is_constant() && !base.values.is_empty() {
                    let addr = add_signed(base.values[0], decoded.imm);
                    if let Some(raw) = instruction_table.read_readonly(addr, decoded.width as usize)
                    {
                        let extended = extend_loaded_value(raw, decoded.width, decoded.is_unsigned);
                        state.set_constant(rd, extended);
                        resolved = true;
                    }
                }
                if !resolved {
                    state.set_unknown(rd);
                }
            }
        }
        InstrKind::Jal | InstrKind::Jalr => {
            if let Some(rd) = decoded.rd {
                state.set_constant(rd, pc + size);
            }
        }
        _ => {
            if let Some(rd) = decoded.rd {
                state.set_unknown(rd);
            }
        }
    }

    state
}

fn compute_leaders<X: Xlen>(
    instruction_table: &InstructionTable<X>,
    successors: &FxHashMap<u64, FxHashSet<u64>>,
    function_entries: &FxHashSet<u64>,
    internal_targets: &FxHashSet<u64>,
    return_sites: &FxHashSet<u64>,
) -> FxHashSet<u64> {
    let mut leaders = FxHashSet::default();
    leaders.extend(function_entries.iter().copied());
    leaders.extend(internal_targets.iter().copied());
    leaders.extend(return_sites.iter().copied());

    for (&pc, succs) in successors {
        let size = instruction_table.instruction_size_at_pc(pc) as u64;
        if size == 0 {
            continue;
        }
        if let Some(instr) = instruction_table.get_at_pc(pc) {
            let decoded = DecodedInstruction::from_instr(instr);
            if decoded.is_control_flow() {
                leaders.extend(succs.iter().copied());
                let next_pc = pc + size;
                if instruction_table.is_valid_pc(next_pc) {
                    leaders.insert(next_pc);
                }
            }
        }
    }

    leaders
}

fn build_predecessors(
    successors: &FxHashMap<u64, FxHashSet<u64>>,
) -> FxHashMap<u64, FxHashSet<u64>> {
    // Build partial maps in parallel, then merge
    successors
        .par_iter()
        .fold(
            FxHashMap::default,
            |mut partial: FxHashMap<u64, FxHashSet<u64>>, (&pc, succs)| {
                for &succ in succs {
                    partial.entry(succ).or_default().insert(pc);
                }
                partial
            },
        )
        .reduce(FxHashMap::default, |mut a, b| {
            for (succ, preds) in b {
                a.entry(succ).or_default().extend(preds);
            }
            a
        })
}

fn scan_ro_segments_for_code_pointers<X: Xlen>(
    instruction_table: &InstructionTable<X>,
    internal_targets: &mut FxHashSet<u64>,
) {
    // Scan for both 4-byte and 8-byte code pointers.
    // Even on RV64, compilers often emit 32-bit jump table entries when
    // addresses fit in 32 bits (common for position-dependent executables).
    for segment in instruction_table.ro_segments() {
        let data = &segment.data;

        // Scan for 4-byte pointers (at 4-byte alignment)
        let mut offset = 0usize;
        while offset + 4 <= data.len() {
            let val = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as u64;
            if instruction_table.is_valid_pc(val) {
                internal_targets.insert(val);
            }
            offset += 4;
        }

        // For 64-bit, also scan for 8-byte pointers (at 8-byte alignment)
        if X::VALUE == 64 {
            let mut offset = 0usize;
            while offset + 8 <= data.len() {
                let val = u64::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                    data[offset + 4],
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]);
                // Only add if the high bits are non-zero (otherwise already caught by 4-byte scan)
                if val > u32::MAX as u64 && instruction_table.is_valid_pc(val) {
                    internal_targets.insert(val);
                }
                offset += 8;
            }
        }
    }
}

fn extract_written_reg(args: &InstrArgs) -> Option<u8> {
    match *args {
        InstrArgs::R { rd, .. }
        | InstrArgs::R4 { rd, .. }
        | InstrArgs::I { rd, .. }
        | InstrArgs::U { rd, .. }
        | InstrArgs::J { rd, .. }
        | InstrArgs::Csr { rd, .. }
        | InstrArgs::CsrI { rd, .. }
        | InstrArgs::Amo { rd, .. } => {
            if rd == 0 {
                None
            } else {
                Some(rd)
            }
        }
        InstrArgs::S { .. } | InstrArgs::B { .. } | InstrArgs::None | InstrArgs::Custom(_) => None,
    }
}

fn add_signed(base: u64, imm: i32) -> u64 {
    let imm = imm as i64 as i128;
    let base = base as i128;
    (base + imm) as u64
}

fn sign_extend_i32(value: i32) -> u64 {
    value as i64 as u64
}

fn extend_loaded_value(value: u64, width: u8, is_unsigned: bool) -> u64 {
    match width {
        1 => {
            let masked = value & 0xFF;
            if is_unsigned {
                masked
            } else if masked & 0x80 != 0 {
                masked | 0xFFFFFFFFFFFFFF00
            } else {
                masked
            }
        }
        2 => {
            let masked = value & 0xFFFF;
            if is_unsigned {
                masked
            } else if masked & 0x8000 != 0 {
                masked | 0xFFFFFFFFFFFF0000
            } else {
                masked
            }
        }
        4 => {
            let masked = value & 0xFFFF_FFFF;
            if is_unsigned {
                masked
            } else if masked & 0x8000_0000 != 0 {
                masked | 0xFFFFFFFF00000000
            } else {
                masked
            }
        }
        _ => value,
    }
}

fn binary_search_le(sorted: &[u64], target: u64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let mut lo = 0usize;
    let mut hi = sorted.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if sorted[mid] <= target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 { None } else { Some(sorted[lo - 1]) }
}
