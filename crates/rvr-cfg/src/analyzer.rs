//! Control flow analyzer using worklist algorithm.

use std::collections::{HashMap, HashSet, VecDeque};

use rvr_elf::ElfImage;
use rvr_isa::Xlen;

use crate::decoder::{CfgInstr, CfgInstrKind};
use crate::value::{RegisterState, RegisterValue};
use crate::CfgResult;

const MAX_ITERATIONS_MULTIPLIER: usize = 20;

/// Code view for accessing instruction bytes and segment info.
pub struct CodeView<'a, X: Xlen> {
    pub image: &'a ElfImage<X>,
    pub entry_point: u64,
    pub base_address: u64,
    pub end_address: u64,
}

impl<'a, X: Xlen> CodeView<'a, X> {
    /// Create a code view from an ELF image.
    pub fn from_image(image: &'a ElfImage<X>) -> Self {
        let mut base = u64::MAX;
        let mut end = 0u64;

        for seg in &image.memory_segments {
            if seg.is_executable() {
                let start = X::to_u64(seg.virtual_start);
                let seg_end = X::to_u64(seg.virtual_end);
                base = base.min(start);
                end = end.max(seg_end);
            }
        }

        Self {
            image,
            entry_point: X::to_u64(image.entry_point),
            base_address: base,
            end_address: end,
        }
    }

    /// Check if PC is within valid code range.
    pub fn is_valid_pc(&self, pc: u64) -> bool {
        pc >= self.base_address && pc < self.end_address
    }

    /// Read instruction bytes at PC. Returns (raw, size).
    pub fn read_instr(&self, pc: u64) -> Option<(u32, u8)> {
        // Find segment containing this PC
        for seg in &self.image.memory_segments {
            let start = X::to_u64(seg.virtual_start);
            let end = X::to_u64(seg.virtual_end);
            if pc >= start && pc < end {
                let offset = (pc - start) as usize;
                if offset >= seg.data.len() {
                    return None; // In BSS
                }

                // Check for compressed instruction
                if offset + 2 > seg.data.len() {
                    return None;
                }
                let low = u16::from_le_bytes([seg.data[offset], seg.data[offset + 1]]);

                // Compressed if low 2 bits != 0b11
                if (low & 0x3) != 0x3 {
                    return Some((low as u32, 2));
                }

                // 4-byte instruction
                if offset + 4 > seg.data.len() {
                    return None;
                }
                let raw = u32::from_le_bytes([
                    seg.data[offset],
                    seg.data[offset + 1],
                    seg.data[offset + 2],
                    seg.data[offset + 3],
                ]);
                return Some((raw, 4));
            }
        }
        None
    }

    /// Read value from read-only segment at address.
    pub fn read_readonly_value(&self, addr: u64, width: usize) -> Option<u64> {
        for seg in &self.image.memory_segments {
            if !seg.is_readonly() {
                continue;
            }
            let start = X::to_u64(seg.virtual_start);
            let end = X::to_u64(seg.virtual_end);
            if addr >= start && addr + width as u64 <= end {
                let offset = (addr - start) as usize;
                if offset + width > seg.data.len() {
                    return None; // In BSS
                }
                let bytes = &seg.data[offset..offset + width];
                return Some(match width {
                    1 => bytes[0] as u64,
                    2 => u16::from_le_bytes([bytes[0], bytes[1]]) as u64,
                    4 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64,
                    8 => u64::from_le_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
                        bytes[7],
                    ]),
                    _ => return None,
                });
            }
        }
        None
    }
}

/// Control flow analyzer.
pub struct CfgAnalyzer<'a, X: Xlen> {
    code: CodeView<'a, X>,
}

impl<'a, X: Xlen> CfgAnalyzer<'a, X> {
    /// Create analyzer from ELF image.
    pub fn new(image: &'a ElfImage<X>) -> Self {
        Self {
            code: CodeView::from_image(image),
        }
    }

    /// Analyze control flow graph.
    pub fn analyze(&self) -> CfgResult {
        // Phase 1: Collect potential targets via value propagation
        let (function_entries, internal_targets, return_sites) = self.collect_potential_targets();

        // Build call_return_map: callee -> return addresses
        let call_return_map = self.build_call_return_map(&function_entries);

        // Pre-sort function entries for binary search
        let mut sorted_function_entries: Vec<u64> = function_entries.iter().copied().collect();
        sorted_function_entries.sort_unstable();

        // Pre-compute per-function internal targets
        let func_internal_targets = self.group_by_function(&sorted_function_entries, &internal_targets);

        // Phase 2: Worklist algorithm
        let successors = self.worklist_analysis(
            &function_entries,
            &internal_targets,
            &return_sites,
            &sorted_function_entries,
            &func_internal_targets,
            &call_return_map,
        );

        // Compute leaders (basic block starts)
        let leaders = self.compute_leaders(
            &successors,
            &function_entries,
            &internal_targets,
            &return_sites,
        );

        // Map blocks to functions
        let block_to_function = self.map_blocks_to_functions(&leaders, &sorted_function_entries);

        // Compute predecessors from successors
        let mut predecessors: HashMap<u64, HashSet<u64>> = HashMap::new();
        for (pc, succs) in &successors {
            for succ in succs {
                predecessors.entry(*succ).or_default().insert(*pc);
            }
        }

        CfgResult {
            successors,
            predecessors,
            unresolved_jumps: HashSet::new(),
            leaders,
            call_return_map,
            block_to_function,
            function_entries,
            internal_targets,
        }
    }

    /// Phase 1: Single pass to collect function entries, internal targets, and return sites.
    fn collect_potential_targets(&self) -> (HashSet<u64>, HashSet<u64>, HashSet<u64>) {
        let mut function_entries = HashSet::new();
        let mut internal_targets = HashSet::new();
        let mut return_sites = HashSet::new();

        // Entry point is a function
        function_entries.insert(self.code.entry_point);

        // Scan read-only segments for code pointers
        self.scan_ro_segments(&mut internal_targets);

        // Simple register state for tracking address computations
        let mut regs: [Option<u64>; 32] = [None; 32];
        regs[0] = Some(0); // x0 = 0

        let mut pc = self.code.base_address;
        while pc < self.code.end_address {
            let Some((raw, size)) = self.code.read_instr(pc) else {
                pc += 4;
                continue;
            };

            let decoded = CfgInstr::decode(raw, size);

            match decoded.kind {
                CfgInstrKind::Lui => {
                    if decoded.rd >= 0 && decoded.rd < 32 {
                        regs[decoded.rd as usize] = Some(decoded.imm as u64);
                    }
                }
                CfgInstrKind::Auipc => {
                    if decoded.rd >= 0 && decoded.rd < 32 {
                        regs[decoded.rd as usize] = Some(add_signed(pc, decoded.imm));
                    }
                }
                CfgInstrKind::Addi => {
                    if decoded.rd >= 0 && decoded.rd < 32 && decoded.rs1 >= 0 && decoded.rs1 < 32 {
                        if let Some(base) = regs[decoded.rs1 as usize] {
                            let computed = add_signed(base, decoded.imm);
                            regs[decoded.rd as usize] = Some(computed);
                            if self.code.is_valid_pc(computed) {
                                function_entries.insert(computed);
                            }
                        } else {
                            regs[decoded.rd as usize] = None;
                        }
                    }
                }
                CfgInstrKind::Add => {
                    if decoded.rd >= 0
                        && decoded.rd < 32
                        && decoded.rs1 >= 0
                        && decoded.rs1 < 32
                        && decoded.rs2 >= 0
                        && decoded.rs2 < 32
                    {
                        let lhs = regs[decoded.rs1 as usize];
                        let rhs = regs[decoded.rs2 as usize];
                        if let (Some(l), Some(r)) = (lhs, rhs) {
                            let computed = l.wrapping_add(r);
                            regs[decoded.rd as usize] = Some(computed);
                            if self.code.is_valid_pc(computed) {
                                function_entries.insert(computed);
                            }
                        } else {
                            regs[decoded.rd as usize] = None;
                        }
                    }
                }
                CfgInstrKind::Move => {
                    if decoded.rd >= 0 && decoded.rd < 32 && decoded.rs1 >= 0 && decoded.rs1 < 32 {
                        regs[decoded.rd as usize] = regs[decoded.rs1 as usize];
                    }
                }
                CfgInstrKind::Load => {
                    if decoded.rd >= 0 && decoded.rd < 32 && decoded.rs1 >= 0 && decoded.rs1 < 32 {
                        if let Some(base) = regs[decoded.rs1 as usize] {
                            let addr = add_signed(base, decoded.imm);
                            if let Some(val) = self
                                .code
                                .read_readonly_value(addr, decoded.width as usize)
                            {
                                let extended = CfgInstr::extend_loaded_value(
                                    val,
                                    decoded.width,
                                    decoded.is_unsigned,
                                );
                                regs[decoded.rd as usize] = Some(extended);
                                if self.code.is_valid_pc(extended) {
                                    internal_targets.insert(extended);
                                }
                            } else {
                                regs[decoded.rd as usize] = None;
                            }
                        } else {
                            regs[decoded.rd as usize] = None;
                        }
                    }
                }
                CfgInstrKind::Jal => {
                    let target = add_signed(pc, decoded.imm);
                    if decoded.is_call() {
                        if self.code.is_valid_pc(target) {
                            function_entries.insert(target);
                        }
                        return_sites.insert(pc + size as u64);
                    } else {
                        if self.code.is_valid_pc(target) {
                            internal_targets.insert(target);
                        }
                    }
                    if decoded.rd >= 0 && decoded.rd < 32 {
                        regs[decoded.rd as usize] = Some(pc + size as u64);
                    }
                }
                CfgInstrKind::Jalr => {
                    // Try to resolve target
                    if decoded.rs1 >= 0 && decoded.rs1 < 32 {
                        if let Some(base) = regs[decoded.rs1 as usize] {
                            let target = (add_signed(base, decoded.imm)) & !1;
                            if self.code.is_valid_pc(target) {
                                function_entries.insert(target);
                            }
                        }
                    }
                    if decoded.is_call() {
                        return_sites.insert(pc + size as u64);
                    }
                    if decoded.rd >= 0 && decoded.rd < 32 {
                        regs[decoded.rd as usize] = Some(pc + size as u64);
                    }
                }
                CfgInstrKind::Branch => {
                    let target = add_signed(pc, decoded.imm);
                    if self.code.is_valid_pc(target) {
                        internal_targets.insert(target);
                    }
                    internal_targets.insert(pc + size as u64);
                }
                _ => {
                    // Unknown instruction that writes rd - clear tracking
                    if decoded.rd >= 0 && decoded.rd < 32 {
                        regs[decoded.rd as usize] = None;
                    }
                }
            }

            pc += size as u64;
        }

        (function_entries, internal_targets, return_sites)
    }

    /// Scan read-only segments for potential code pointers.
    fn scan_ro_segments(&self, internal_targets: &mut HashSet<u64>) {
        for seg in &self.code.image.memory_segments {
            if !seg.is_readonly() || seg.is_executable() {
                continue;
            }

            let ptr_size = if X::VALUE == 64 { 8 } else { 4 };
            let data_size = seg.data.len();

            let mut offset = 0;
            while offset + ptr_size <= data_size {
                let val = if ptr_size == 8 {
                    u64::from_le_bytes([
                        seg.data[offset],
                        seg.data[offset + 1],
                        seg.data[offset + 2],
                        seg.data[offset + 3],
                        seg.data[offset + 4],
                        seg.data[offset + 5],
                        seg.data[offset + 6],
                        seg.data[offset + 7],
                    ])
                } else {
                    u32::from_le_bytes([
                        seg.data[offset],
                        seg.data[offset + 1],
                        seg.data[offset + 2],
                        seg.data[offset + 3],
                    ]) as u64
                };

                if self.code.is_valid_pc(val) {
                    internal_targets.insert(val);
                }
                offset += ptr_size;
            }
        }
    }

    /// Build map of callee -> return addresses.
    fn build_call_return_map(&self, function_entries: &HashSet<u64>) -> HashMap<u64, HashSet<u64>> {
        let mut call_return_map: HashMap<u64, HashSet<u64>> = HashMap::new();

        let mut pc = self.code.base_address;
        while pc < self.code.end_address {
            let Some((raw, size)) = self.code.read_instr(pc) else {
                pc += 4;
                continue;
            };

            let decoded = CfgInstr::decode(raw, size);
            if decoded.is_static_call() {
                let callee = add_signed(pc, decoded.imm);
                if self.code.is_valid_pc(callee) && function_entries.contains(&callee) {
                    let return_addr = pc + size as u64;
                    call_return_map.entry(callee).or_default().insert(return_addr);
                }
            }

            pc += size as u64;
        }

        call_return_map
    }

    /// Group internal targets by containing function.
    fn group_by_function(
        &self,
        sorted_functions: &[u64],
        targets: &HashSet<u64>,
    ) -> HashMap<u64, HashSet<u64>> {
        let mut result: HashMap<u64, HashSet<u64>> = HashMap::new();
        for &target in targets {
            let func = binary_search_le(sorted_functions, target);
            result.entry(func).or_default().insert(target);
        }
        result
    }

    /// Phase 2: Worklist algorithm for CFG construction.
    fn worklist_analysis(
        &self,
        function_entries: &HashSet<u64>,
        internal_targets: &HashSet<u64>,
        return_sites: &HashSet<u64>,
        sorted_function_entries: &[u64],
        func_internal_targets: &HashMap<u64, HashSet<u64>>,
        call_return_map: &HashMap<u64, HashSet<u64>>,
    ) -> HashMap<u64, HashSet<u64>> {
        let mut states: HashMap<u64, RegisterState> = HashMap::new();
        let mut worklist: VecDeque<u64> = VecDeque::new();
        let mut in_worklist: HashSet<u64> = HashSet::new();
        let mut successors: HashMap<u64, HashSet<u64>> = HashMap::new();

        // Seed worklist
        let entry = self.code.entry_point;
        states.insert(entry, RegisterState::new());
        worklist.push_back(entry);
        in_worklist.insert(entry);

        for &addr in function_entries {
            if !in_worklist.contains(&addr) {
                states.insert(addr, RegisterState::new());
                worklist.push_back(addr);
                in_worklist.insert(addr);
            }
        }

        for &addr in internal_targets {
            if !in_worklist.contains(&addr) {
                states.insert(addr, RegisterState::new());
                worklist.push_back(addr);
                in_worklist.insert(addr);
            }
        }

        let max_iterations =
            (self.code.end_address - self.code.base_address) as usize * MAX_ITERATIONS_MULTIPLIER;
        let mut iterations = 0;

        while let Some(pc) = worklist.pop_front() {
            iterations += 1;
            if iterations > max_iterations {
                break;
            }

            in_worklist.remove(&pc);

            let Some(state) = states.get(&pc).cloned() else {
                continue;
            };

            let Some((raw, size)) = self.code.read_instr(pc) else {
                continue;
            };

            let decoded = CfgInstr::decode(raw, size);

            let succs = self.get_successors(
                pc,
                size,
                &decoded,
                &state,
                function_entries,
                return_sites,
                sorted_function_entries,
                func_internal_targets,
                call_return_map,
            );

            let state_out = self.transfer(pc, size, &decoded, state);

            for &target in &succs {
                if let Some(existing) = states.get_mut(&target) {
                    if existing.merge(&state_out) && !in_worklist.contains(&target) {
                        worklist.push_back(target);
                        in_worklist.insert(target);
                    }
                } else {
                    states.insert(target, state_out.clone());
                    if !in_worklist.contains(&target) {
                        worklist.push_back(target);
                        in_worklist.insert(target);
                    }
                }
            }

            successors.entry(pc).or_default().extend(succs);
        }

        successors
    }

    /// Get successors for an instruction.
    fn get_successors(
        &self,
        pc: u64,
        size: u8,
        decoded: &CfgInstr,
        state: &RegisterState,
        function_entries: &HashSet<u64>,
        return_sites: &HashSet<u64>,
        sorted_function_entries: &[u64],
        func_internal_targets: &HashMap<u64, HashSet<u64>>,
        call_return_map: &HashMap<u64, HashSet<u64>>,
    ) -> HashSet<u64> {
        let mut result = HashSet::new();

        match decoded.kind {
            CfgInstrKind::Jal => {
                let target = add_signed(pc, decoded.imm);
                if self.code.is_valid_pc(target) {
                    result.insert(target);
                }
                if decoded.is_call() {
                    result.insert(pc + size as u64);
                }
            }
            CfgInstrKind::Jalr => {
                if decoded.rs1 >= 0 && decoded.rs1 < 32 {
                    let base = state.get(decoded.rs1 as usize);
                    if let Some(values) = base.values() {
                        // Resolved - use computed targets
                        for &v in values {
                            let target = (add_signed(v, decoded.imm)) & !1;
                            if self.code.is_valid_pc(target) {
                                result.insert(target);
                            }
                        }
                        if decoded.is_call() {
                            result.insert(pc + size as u64);
                        }
                    } else if decoded.is_return() {
                        // Context-sensitive return
                        let func_start = binary_search_le(sorted_function_entries, pc);
                        if let Some(returns) = call_return_map.get(&func_start) {
                            result.extend(returns);
                        } else {
                            // Function only reachable via indirect call - conservative
                            result.extend(return_sites);
                        }
                    } else if decoded.is_call() {
                        // Indirect call: any function entry + fall through
                        result.extend(function_entries);
                        result.insert(pc + size as u64);
                    } else if decoded.is_indirect_jump() {
                        // Indirect jump: internal targets + function entries (tail calls)
                        let func_start = binary_search_le(sorted_function_entries, pc);
                        if let Some(targets) = func_internal_targets.get(&func_start) {
                            result.extend(targets);
                        }
                        result.extend(function_entries);
                    }
                }
            }
            CfgInstrKind::Branch => {
                result.insert(pc + size as u64);
                let target = add_signed(pc, decoded.imm);
                if self.code.is_valid_pc(target) {
                    result.insert(target);
                }
            }
            _ => {
                result.insert(pc + size as u64);
            }
        }

        result
    }

    /// Transfer function - update register state after instruction.
    fn transfer(&self, pc: u64, size: u8, decoded: &CfgInstr, mut state: RegisterState) -> RegisterState {
        match decoded.kind {
            CfgInstrKind::Lui => {
                if decoded.rd > 0 && decoded.rd < 32 {
                    state.set_constant(decoded.rd as usize, decoded.imm as u64);
                }
            }
            CfgInstrKind::Auipc => {
                if decoded.rd > 0 && decoded.rd < 32 {
                    state.set_constant(decoded.rd as usize, add_signed(pc, decoded.imm));
                }
            }
            CfgInstrKind::Addi => {
                if decoded.rd > 0 && decoded.rd < 32 && decoded.rs1 >= 0 && decoded.rs1 < 32 {
                    let base = state.get(decoded.rs1 as usize);
                    if let Some(values) = base.values() {
                        let mut result = RegisterValue::constant(add_signed(values[0], decoded.imm));
                        for &v in &values[1..] {
                            result.add_value(add_signed(v, decoded.imm));
                        }
                        state.set(decoded.rd as usize, result);
                    } else {
                        state.set_unknown(decoded.rd as usize);
                    }
                }
            }
            CfgInstrKind::Add => {
                if decoded.rd > 0
                    && decoded.rd < 32
                    && decoded.rs1 >= 0
                    && decoded.rs1 < 32
                    && decoded.rs2 >= 0
                    && decoded.rs2 < 32
                {
                    let lhs = state.get(decoded.rs1 as usize);
                    let rhs = state.get(decoded.rs2 as usize);
                    if let (Some(lv), Some(rv)) = (lhs.values(), rhs.values()) {
                        let mut result = RegisterValue::constant(lv[0].wrapping_add(rv[0]));
                        'outer: for (i, &l) in lv.iter().enumerate() {
                            for (j, &r) in rv.iter().enumerate() {
                                if i == 0 && j == 0 {
                                    continue;
                                }
                                result.add_value(l.wrapping_add(r));
                                if !result.is_constant() {
                                    break 'outer;
                                }
                            }
                        }
                        state.set(decoded.rd as usize, result);
                    } else {
                        state.set_unknown(decoded.rd as usize);
                    }
                }
            }
            CfgInstrKind::Move => {
                if decoded.rd > 0 && decoded.rd < 32 && decoded.rs1 >= 0 && decoded.rs1 < 32 {
                    let val = state.get(decoded.rs1 as usize).clone();
                    state.set(decoded.rd as usize, val);
                }
            }
            CfgInstrKind::Load => {
                if decoded.rd > 0 && decoded.rd < 32 && decoded.rs1 >= 0 && decoded.rs1 < 32 {
                    // Skip stack loads (sp = x2)
                    let base = state.get(decoded.rs1 as usize);
                    if decoded.rs1 != 2 {
                        if let Some(values) = base.values() {
                            if !values.is_empty() {
                                let addr = add_signed(values[0], decoded.imm);
                                if let Some(val) = self.code.read_readonly_value(addr, decoded.width as usize) {
                                    let extended = CfgInstr::extend_loaded_value(val, decoded.width, decoded.is_unsigned);
                                    state.set_constant(decoded.rd as usize, extended);
                                    return state;
                                }
                            }
                        }
                    }
                    state.set_unknown(decoded.rd as usize);
                }
            }
            CfgInstrKind::Jal | CfgInstrKind::Jalr => {
                if decoded.rd > 0 && decoded.rd < 32 {
                    state.set_constant(decoded.rd as usize, pc + size as u64);
                }
            }
            _ => {
                if decoded.rd > 0 && decoded.rd < 32 {
                    state.set_unknown(decoded.rd as usize);
                }
            }
        }

        state
    }

    /// Compute basic block leaders.
    fn compute_leaders(
        &self,
        successors: &HashMap<u64, HashSet<u64>>,
        function_entries: &HashSet<u64>,
        internal_targets: &HashSet<u64>,
        return_sites: &HashSet<u64>,
    ) -> HashSet<u64> {
        let mut leaders = HashSet::new();

        leaders.insert(self.code.entry_point);
        leaders.extend(function_entries);
        leaders.extend(internal_targets);
        leaders.extend(return_sites);

        // Add targets from control flow instructions
        for (&pc, succs) in successors {
            let Some((raw, size)) = self.code.read_instr(pc) else {
                continue;
            };
            let decoded = CfgInstr::decode(raw, size);

            if decoded.is_control_flow() {
                leaders.extend(succs);
                let next_pc = pc + size as u64;
                if self.code.is_valid_pc(next_pc) {
                    leaders.insert(next_pc);
                }
            }
        }

        leaders
    }

    /// Map each block to its containing function.
    fn map_blocks_to_functions(
        &self,
        leaders: &HashSet<u64>,
        sorted_functions: &[u64],
    ) -> HashMap<u64, u64> {
        let mut result = HashMap::new();
        for &leader in leaders {
            let func = binary_search_le(sorted_functions, leader);
            if func != 0 {
                result.insert(leader, func);
            }
        }
        result
    }
}

/// Add signed immediate to address.
fn add_signed(base: u64, imm: i32) -> u64 {
    (base as i64).wrapping_add(imm as i64) as u64
}

/// Binary search for largest value <= target.
fn binary_search_le(sorted: &[u64], target: u64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    match sorted.binary_search(&target) {
        Ok(i) => sorted[i],
        Err(0) => 0,
        Err(i) => sorted[i - 1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_signed() {
        assert_eq!(add_signed(0x1000, 8), 0x1008);
        assert_eq!(add_signed(0x1000, -8), 0x0FF8);
    }

    #[test]
    fn test_binary_search_le() {
        let sorted = vec![10, 20, 30, 40, 50];
        assert_eq!(binary_search_le(&sorted, 25), 20);
        assert_eq!(binary_search_le(&sorted, 30), 30);
        assert_eq!(binary_search_le(&sorted, 5), 0);
        assert_eq!(binary_search_le(&sorted, 55), 50);
    }
}
