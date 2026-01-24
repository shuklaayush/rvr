//! Control flow analysis used to identify basic block leaders and targets.

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace, trace_span, warn};

use rvr_isa::{
    DecodedInstr, InstrArgs, OP_ADD, OP_ADDI, OP_AUIPC, OP_BEQ, OP_BGE, OP_BGEU, OP_BLT, OP_BLTU,
    OP_BNE, OP_C_ADD, OP_C_ADDI, OP_C_ADDI4SPN, OP_C_ADDI16SP, OP_C_BEQZ, OP_C_BNEZ, OP_C_J,
    OP_C_JAL, OP_C_JALR, OP_C_JR, OP_C_LD, OP_C_LDSP, OP_C_LI, OP_C_LUI, OP_C_LW, OP_C_LWSP,
    OP_C_MV, OP_JAL, OP_JALR, OP_LB, OP_LBU, OP_LD, OP_LH, OP_LHU, OP_LUI, OP_LW, OP_LWU, Xlen,
};

use crate::InstructionTable;

const NUM_REGS: usize = 32;
const MAX_VALUES: usize = 16;
const MAX_ITERATIONS_MULTIPLIER: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueKind {
    Unknown,
    Constant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RegisterValue {
    kind: ValueKind,
    values: Vec<u64>,
}

impl RegisterValue {
    fn unknown() -> Self {
        Self {
            kind: ValueKind::Unknown,
            values: Vec::new(),
        }
    }

    fn constant(value: u64) -> Self {
        Self {
            kind: ValueKind::Constant,
            values: vec![value],
        }
    }

    fn is_constant(&self) -> bool {
        self.kind == ValueKind::Constant
    }

    fn add_value(&mut self, value: u64) {
        if self.kind != ValueKind::Constant {
            return;
        }

        match self.values.binary_search(&value) {
            Ok(_) => {}
            Err(idx) => {
                if self.values.len() >= MAX_VALUES {
                    self.kind = ValueKind::Unknown;
                    self.values.clear();
                } else {
                    self.values.insert(idx, value);
                }
            }
        }
    }

    fn merge(&self, other: &Self) -> Self {
        if self.kind == ValueKind::Unknown || other.kind == ValueKind::Unknown {
            return Self::unknown();
        }

        let mut merged = Vec::with_capacity(self.values.len() + other.values.len());
        let mut i = 0;
        let mut j = 0;

        while i < self.values.len() && j < other.values.len() {
            let a = self.values[i];
            let b = other.values[j];
            if a == b {
                merged.push(a);
                i += 1;
                j += 1;
            } else if a < b {
                merged.push(a);
                i += 1;
            } else {
                merged.push(b);
                j += 1;
            }

            if merged.len() > MAX_VALUES {
                return Self::unknown();
            }
        }

        while i < self.values.len() {
            merged.push(self.values[i]);
            i += 1;
            if merged.len() > MAX_VALUES {
                return Self::unknown();
            }
        }

        while j < other.values.len() {
            merged.push(other.values[j]);
            j += 1;
            if merged.len() > MAX_VALUES {
                return Self::unknown();
            }
        }

        Self {
            kind: ValueKind::Constant,
            values: merged,
        }
    }
}

#[derive(Clone, Debug)]
struct RegisterState {
    regs: [RegisterValue; NUM_REGS],
}

impl RegisterState {
    fn new() -> Self {
        let mut regs = std::array::from_fn(|_| RegisterValue::unknown());
        regs[0] = RegisterValue::constant(0);
        Self { regs }
    }

    fn get(&self, reg: u8) -> RegisterValue {
        let idx = reg as usize;
        if idx >= NUM_REGS {
            return RegisterValue::unknown();
        }
        if idx == 0 {
            return RegisterValue::constant(0);
        }
        self.regs[idx].clone()
    }

    fn get_ref(&self, reg: u8) -> &RegisterValue {
        let idx = reg as usize;
        if idx >= NUM_REGS {
            return &self.regs[0];
        }
        &self.regs[idx]
    }

    fn set(&mut self, reg: u8, value: RegisterValue) {
        let idx = reg as usize;
        if idx == 0 || idx >= NUM_REGS {
            return;
        }
        self.regs[idx] = value;
    }

    fn set_constant(&mut self, reg: u8, value: u64) {
        self.set(reg, RegisterValue::constant(value));
    }

    fn set_unknown(&mut self, reg: u8) {
        self.set(reg, RegisterValue::unknown());
    }

    fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for idx in 1..NUM_REGS {
            let merged = self.regs[idx].merge(&other.regs[idx]);
            if merged != self.regs[idx] {
                self.regs[idx] = merged;
                changed = true;
            }
        }
        changed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstrKind {
    Unknown,
    Lui,
    Auipc,
    Addi,
    Add,
    Move,
    Jal,
    Jalr,
    Load,
    Branch,
}

#[derive(Clone, Copy, Debug)]
struct DecodedInstruction {
    kind: InstrKind,
    rd: Option<u8>,
    rs1: Option<u8>,
    rs2: Option<u8>,
    imm: i32,
    width: u8,
    is_unsigned: bool,
}

impl DecodedInstruction {
    fn unknown() -> Self {
        Self {
            kind: InstrKind::Unknown,
            rd: None,
            rs1: None,
            rs2: None,
            imm: 0,
            width: 0,
            is_unsigned: false,
        }
    }

    fn from_instr<X: Xlen>(instr: &DecodedInstr<X>) -> Self {
        let opid = instr.opid;
        match opid {
            OP_LUI | OP_C_LUI => match instr.args.clone() {
                InstrArgs::U { rd, imm } => Self {
                    kind: InstrKind::Lui,
                    rd: Some(rd),
                    rs1: None,
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_AUIPC => match instr.args.clone() {
                InstrArgs::U { rd, imm } => Self {
                    kind: InstrKind::Auipc,
                    rd: Some(rd),
                    rs1: None,
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_ADDI | OP_C_ADDI | OP_C_ADDI16SP | OP_C_ADDI4SPN | OP_C_LI => {
                match instr.args.clone() {
                    InstrArgs::I { rd, rs1, imm } => Self {
                        kind: InstrKind::Addi,
                        rd: Some(rd),
                        rs1: Some(rs1),
                        rs2: None,
                        imm,
                        width: 0,
                        is_unsigned: false,
                    },
                    _ => Self::unknown(),
                }
            }
            OP_ADD | OP_C_ADD => match instr.args.clone() {
                InstrArgs::R { rd, rs1, rs2 } => Self {
                    kind: InstrKind::Add,
                    rd: Some(rd),
                    rs1: Some(rs1),
                    rs2: Some(rs2),
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_C_MV => match instr.args.clone() {
                InstrArgs::R { rd, rs2, .. } => Self {
                    kind: InstrKind::Move,
                    rd: Some(rd),
                    rs1: Some(rs2),
                    rs2: None,
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_JAL | OP_C_J | OP_C_JAL => match instr.args.clone() {
                InstrArgs::J { rd, imm } => Self {
                    kind: InstrKind::Jal,
                    rd: Some(rd),
                    rs1: None,
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_JALR | OP_C_JR | OP_C_JALR => match instr.args.clone() {
                InstrArgs::I { rd, rs1, imm } => Self {
                    kind: InstrKind::Jalr,
                    rd: Some(rd),
                    rs1: Some(rs1),
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_LB | OP_LBU | OP_LH | OP_LHU | OP_LW | OP_LWU | OP_LD | OP_C_LW | OP_C_LWSP
            | OP_C_LD | OP_C_LDSP => match instr.args.clone() {
                InstrArgs::I { rd, rs1, imm } => {
                    let (width, is_unsigned) = match opid {
                        OP_LB => (1, false),
                        OP_LBU => (1, true),
                        OP_LH => (2, false),
                        OP_LHU => (2, true),
                        OP_LW => (4, false),
                        OP_LWU => (4, true),
                        OP_LD => (8, false),
                        OP_C_LW | OP_C_LWSP => (4, false),
                        OP_C_LD | OP_C_LDSP => (8, false),
                        _ => (0, false),
                    };
                    Self {
                        kind: InstrKind::Load,
                        rd: Some(rd),
                        rs1: Some(rs1),
                        rs2: None,
                        imm,
                        width,
                        is_unsigned,
                    }
                }
                _ => Self::unknown(),
            },
            OP_BEQ | OP_BNE | OP_BLT | OP_BGE | OP_BLTU | OP_BGEU | OP_C_BEQZ | OP_C_BNEZ => {
                match instr.args.clone() {
                    InstrArgs::B { rs1, rs2, imm } => Self {
                        kind: InstrKind::Branch,
                        rd: None,
                        rs1: Some(rs1),
                        rs2: Some(rs2),
                        imm,
                        width: 0,
                        is_unsigned: false,
                    },
                    _ => Self::unknown(),
                }
            }
            _ => {
                let rd = extract_written_reg(&instr.args);
                let mut decoded = Self::unknown();
                decoded.rd = rd;
                decoded
            }
        }
    }

    fn is_control_flow(&self) -> bool {
        matches!(
            self.kind,
            InstrKind::Jal | InstrKind::Jalr | InstrKind::Branch
        )
    }

    fn is_static_call(&self) -> bool {
        self.kind == InstrKind::Jal && self.rd != Some(0)
    }

    fn is_call(&self) -> bool {
        match self.kind {
            InstrKind::Jal | InstrKind::Jalr => self.rd != Some(0),
            _ => false,
        }
    }

    fn is_return(&self) -> bool {
        self.kind == InstrKind::Jalr && self.rd == Some(0) && self.rs1 == Some(1)
    }

    fn is_indirect_jump(&self) -> bool {
        self.kind == InstrKind::Jalr && self.rd == Some(0) && self.rs1 != Some(1)
    }
}

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

        // Unresolved indirect jumps are handled conservatively by connecting them
        // to all function entries and internal targets - no warning needed
        if !unresolved_dynamic_jumps.is_empty() {
            debug!(
                count = unresolved_dynamic_jumps.len(),
                "indirect jumps handled conservatively (targets over-approximated)"
            );
        }

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
                if let Some(rs1) = decoded.rs1 {
                    if let Some(base) = regs[rs1 as usize] {
                        let target = add_signed(base, decoded.imm) & !1u64;
                        if instruction_table.is_valid_pc(target) {
                            function_entries.insert(target);
                        }
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
                // Only track true indirect jumps (not returns, not calls) as unresolved
                // Returns and indirect calls are handled conservatively and are fine
                if decoded.is_indirect_jump() {
                    unresolved_dynamic_jumps.insert(pc);
                }
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
                    if let Some(func_start) = binary_search_le(sorted_function_entries, pc) {
                        if let Some(targets) = func_internal_targets.get(&func_start) {
                            result.extend(targets.iter().copied());
                        }
                    }
                    result.extend(function_entries.iter().copied());
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
