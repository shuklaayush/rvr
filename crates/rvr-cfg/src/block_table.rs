//! Block table for CFG analysis and block transforms.
//!
//! Based on Mojo's BlockTable with support for merge, tail-dup, and superblock transforms.

use std::collections::{HashMap, HashSet};

use rvr_isa::{ExtensionRegistry, Xlen};

use crate::InstructionTable;

/// Basic block with start/end addresses.
#[derive(Clone, Debug)]
pub struct BasicBlock {
    /// Starting PC.
    pub start: u64,
    /// Ending PC (exclusive).
    pub end: u64,
    /// Number of instructions in this block.
    pub instruction_count: usize,
    /// PC of the last instruction.
    pub last_pc: u64,
}

impl BasicBlock {
    pub fn new(start: u64, end: u64, instruction_count: usize, last_pc: u64) -> Self {
        Self {
            start,
            end,
            instruction_count,
            last_pc,
        }
    }

    /// Size of block in bytes.
    pub fn size(&self) -> u64 {
        self.end - self.start
    }
}

/// Block table with CFG analysis and transforms.
pub struct BlockTable<X: Xlen> {
    /// List of basic blocks.
    pub blocks: Vec<BasicBlock>,
    /// Absorbed PC -> merged block start mapping (for dispatch table).
    pub absorbed_to_merged: HashMap<u64, u64>,
    /// Block continuations: merged_start -> list of (start, end) ranges.
    pub block_continuations: HashMap<u64, Vec<(u64, u64)>>,
    /// Taken path inlines: branch_pc -> (inline_start, inline_end).
    pub taken_inlines: HashMap<u64, (u64, u64)>,
    /// Predecessors map: PC -> set of predecessor PCs.
    pub predecessors: HashMap<u64, HashSet<u64>>,
    /// Unresolved dynamic jumps.
    pub unresolved_jumps: HashSet<u64>,
    /// Call return map: callee -> set of return addresses.
    pub call_return_map: HashMap<u64, HashSet<u64>>,
    /// Block to function mapping: block_start -> function_entry.
    pub block_to_function: HashMap<u64, u64>,
    /// Reference to instruction table.
    instruction_table: InstructionTable<X>,
}

/// Default limits for block transforms.
pub const DEFAULT_SUPERBLOCK_DEPTH: usize = 100;
pub const DEFAULT_TAIL_DUP_SIZE: usize = 100;
pub const DEFAULT_TAKEN_INLINE_SIZE: usize = 50;

impl<X: Xlen> BlockTable<X> {
    /// Create a new block table from an instruction table with CFG analysis.
    pub fn from_instruction_table(
        instruction_table: InstructionTable<X>,
        registry: &ExtensionRegistry<X>,
    ) -> Self {
        let mut table = Self {
            blocks: Vec::new(),
            absorbed_to_merged: HashMap::new(),
            block_continuations: HashMap::new(),
            taken_inlines: HashMap::new(),
            predecessors: HashMap::new(),
            unresolved_jumps: HashSet::new(),
            call_return_map: HashMap::new(),
            block_to_function: HashMap::new(),
            instruction_table,
        };
        table.build_blocks(registry);
        table
    }

    /// Create a block table with linear blocks (one instruction per block).
    pub fn linear(instruction_table: InstructionTable<X>) -> Self {
        let mut table = Self {
            blocks: Vec::new(),
            absorbed_to_merged: HashMap::new(),
            block_continuations: HashMap::new(),
            taken_inlines: HashMap::new(),
            predecessors: HashMap::new(),
            unresolved_jumps: HashSet::new(),
            call_return_map: HashMap::new(),
            block_to_function: HashMap::new(),
            instruction_table,
        };
        table.build_linear_blocks();
        table
    }

    /// Build linear blocks (one instruction per block).
    fn build_linear_blocks(&mut self) {
        let base = self.instruction_table.base_address();
        let end = self.instruction_table.end_address();
        let mut pc = base;

        while pc < end {
            if !self.instruction_table.is_valid_pc(pc) {
                pc += 2; // Skip to next slot
                continue;
            }
            let size = self.instruction_table.instruction_size_at_pc(pc) as u64;
            if size == 0 {
                pc += 2;
                continue;
            }
            self.blocks.push(BasicBlock::new(pc, pc + size, 1, pc));
            pc += size;
        }
    }

    /// Build blocks using CFG analysis.
    fn build_blocks(&mut self, registry: &ExtensionRegistry<X>) {
        // Find block leaders
        let leaders = self.find_leaders(registry);

        // Create blocks from leaders
        self.create_blocks_from_leaders(&leaders, registry);
    }

    /// Find basic block leaders.
    fn find_leaders(&mut self, registry: &ExtensionRegistry<X>) -> HashSet<u64> {
        let mut leaders = HashSet::new();
        let _base = self.instruction_table.base_address();
        let end = self.instruction_table.end_address();

        // Entry point is always a leader
        leaders.insert(self.instruction_table.entry_point());

        // Find all branch/jump targets
        for (pc, instr) in self.instruction_table.valid_instructions() {
            let ir = registry.lift(instr);
            let next_pc = pc + instr.size as u64;

            // Add predecessor mapping
            match &ir.terminator {
                rvr_ir::Terminator::Fall { .. } => {
                    if next_pc < end {
                        self.predecessors
                            .entry(next_pc)
                            .or_default()
                            .insert(pc);
                    }
                }
                rvr_ir::Terminator::Jump { target } => {
                    let target_pc = X::to_u64(*target);
                    leaders.insert(target_pc);
                    self.predecessors
                        .entry(target_pc)
                        .or_default()
                        .insert(pc);
                }
                rvr_ir::Terminator::JumpDyn { resolved, .. } => {
                    if let Some(targets) = resolved {
                        for target in targets {
                            let target_pc = X::to_u64(*target);
                            leaders.insert(target_pc);
                            self.predecessors
                                .entry(target_pc)
                                .or_default()
                                .insert(pc);
                        }
                    } else {
                        self.unresolved_jumps.insert(pc);
                    }
                }
                rvr_ir::Terminator::Branch { target, .. } => {
                    let target_pc = X::to_u64(*target);
                    leaders.insert(target_pc);
                    self.predecessors
                        .entry(target_pc)
                        .or_default()
                        .insert(pc);

                    // Fall-through is also a leader
                    if next_pc < end {
                        leaders.insert(next_pc);
                        self.predecessors
                            .entry(next_pc)
                            .or_default()
                            .insert(pc);
                    }
                }
                _ => {}
            }
        }

        leaders
    }

    /// Create blocks from leader set.
    fn create_blocks_from_leaders(
        &mut self,
        leaders: &HashSet<u64>,
        registry: &ExtensionRegistry<X>,
    ) {
        // Sort leaders
        let mut sorted_leaders: Vec<u64> = leaders.iter().copied().collect();
        sorted_leaders.sort();

        let end = self.instruction_table.end_address();

        for (i, &block_start) in sorted_leaders.iter().enumerate() {
            if !self.instruction_table.is_valid_pc(block_start) {
                continue;
            }

            // Find max end PC (next leader or table end)
            let max_end = sorted_leaders
                .get(i + 1)
                .copied()
                .unwrap_or(end)
                .min(end);

            let mut pc = block_start;
            let mut instruction_count = 0;
            let mut last_pc = block_start;

            while pc < max_end && pc < end {
                if !self.instruction_table.is_valid_pc(pc) {
                    break;
                }

                let size = self.instruction_table.instruction_size_at_pc(pc) as u64;
                if size == 0 {
                    break;
                }

                instruction_count += 1;
                last_pc = pc;

                // Check if this instruction ends the block
                if let Some(instr) = self.instruction_table.get_at_pc(pc) {
                    let ir = registry.lift(instr);
                    if ir.terminator.is_control_flow() {
                        pc += size;
                        break;
                    }
                }

                let next_pc = pc + size;

                // Stop before reaching next leader
                if leaders.contains(&next_pc) && next_pc != block_start {
                    pc = next_pc;
                    break;
                }

                pc = next_pc;
            }

            if instruction_count > 0 && pc > block_start {
                self.blocks.push(BasicBlock::new(
                    block_start,
                    pc,
                    instruction_count,
                    last_pc,
                ));
            }
        }
    }

    /// Get instruction table reference.
    pub fn instruction_table(&self) -> &InstructionTable<X> {
        &self.instruction_table
    }

    /// Get number of blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Get block by index.
    pub fn get(&self, index: usize) -> Option<&BasicBlock> {
        self.blocks.get(index)
    }

    /// Iterate over blocks.
    pub fn iter(&self) -> impl Iterator<Item = &BasicBlock> {
        self.blocks.iter()
    }

    // ============= Block Transforms =============

    /// Merge blocks where successor has single predecessor.
    ///
    /// Returns number of blocks absorbed.
    pub fn merge_blocks(&mut self, registry: &ExtensionRegistry<X>) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }

        let entry_point = self.instruction_table.entry_point();

        // Build lookup map
        let start_to_idx: HashMap<u64, usize> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.start, i))
            .collect();

        // Find absorbable blocks
        let mut absorbed = HashSet::new();
        for block in &self.blocks {
            if absorbed.contains(&block.start) {
                continue;
            }
            if let Some(target) = self.get_merge_target(block, entry_point, registry) {
                absorbed.insert(target);
            }
        }

        if absorbed.is_empty() {
            return 0;
        }

        // Build merged blocks with continuation chains
        let mut merged = Vec::new();
        self.absorbed_to_merged.clear();
        self.block_continuations.clear();

        for block in &self.blocks {
            if absorbed.contains(&block.start) {
                continue;
            }

            let mut continuations = Vec::new();
            let mut count = block.instruction_count;
            let mut last_pc = block.last_pc;
            let mut current_block = block.clone();

            // Follow continuation chain
            loop {
                let target = self.get_merge_target(&current_block, entry_point, registry);
                match target {
                    Some(target_pc) if absorbed.contains(&target_pc) => {
                        if let Some(&target_idx) = start_to_idx.get(&target_pc) {
                            let target_block = &self.blocks[target_idx];

                            self.absorbed_to_merged.insert(target_pc, block.start);
                            continuations.push((target_block.start, target_block.end));
                            count += target_block.instruction_count;
                            last_pc = target_block.last_pc;
                            current_block = target_block.clone();
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }

            if !continuations.is_empty() {
                self.block_continuations.insert(block.start, continuations.clone());
            }

            // Keep original end - continuations handle absorbed blocks
            merged.push(BasicBlock::new(block.start, block.end, count, last_pc));
        }

        let absorbed_count = self.blocks.len() - merged.len();
        self.blocks = merged;
        absorbed_count
    }

    /// Get merge target if block can merge with its successor.
    fn get_merge_target(
        &self,
        block: &BasicBlock,
        entry_point: u64,
        registry: &ExtensionRegistry<X>,
    ) -> Option<u64> {
        let instr = self.instruction_table.get_at_pc(block.last_pc)?;
        let ir = registry.lift(instr);

        let target_pc = match &ir.terminator {
            rvr_ir::Terminator::Fall { target } => target.map(|t| X::to_u64(t))?,
            rvr_ir::Terminator::Jump { target } => X::to_u64(*target),
            _ => return None,
        };

        if target_pc == entry_point {
            return None;
        }

        // Must have exactly one predecessor (this block's last instruction)
        let preds = self.predecessors.get(&target_pc)?;
        if preds.len() != 1 || !preds.contains(&block.last_pc) {
            return None;
        }

        Some(target_pc)
    }

    /// Duplicate small blocks with multiple predecessors into each predecessor.
    ///
    /// Returns number of blocks eliminated.
    pub fn tail_duplicate(
        &mut self,
        max_dup_size: usize,
        registry: &ExtensionRegistry<X>,
    ) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }

        let entry_point = self.instruction_table.entry_point();

        // Build lookup maps
        let start_to_idx: HashMap<u64, usize> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.start, i))
            .collect();

        let last_pc_to_block_start: HashMap<u64, u64> =
            self.blocks.iter().map(|b| (b.last_pc, b.start)).collect();

        // Find blocks eligible for tail duplication
        let mut to_duplicate = HashSet::new();

        for block in &self.blocks {
            // Skip entry point
            if block.start == entry_point {
                continue;
            }

            // Skip blocks that are too large
            if block.instruction_count > max_dup_size {
                continue;
            }

            // Must have multiple predecessors (join point)
            let preds = match self.predecessors.get(&block.start) {
                Some(p) if p.len() >= 2 => p,
                _ => continue,
            };

            // All predecessors must end with unconditional control flow
            let all_unconditional = preds.iter().all(|&pred_pc| {
                if let Some(instr) = self.instruction_table.get_at_pc(pred_pc) {
                    let ir = registry.lift(instr);
                    matches!(
                        ir.terminator,
                        rvr_ir::Terminator::Fall { .. } | rvr_ir::Terminator::Jump { .. }
                    )
                } else {
                    false
                }
            });

            if all_unconditional {
                to_duplicate.insert(block.start);
            }
        }

        if to_duplicate.is_empty() {
            return 0;
        }

        // For each block to duplicate, add it to each predecessor's continuations
        for &dup_start in &to_duplicate {
            let dup_idx = match start_to_idx.get(&dup_start) {
                Some(&idx) => idx,
                None => continue,
            };
            let dup_block = &self.blocks[dup_idx];

            let preds = match self.predecessors.get(&dup_start) {
                Some(p) => p.clone(),
                None => continue,
            };

            let mut first_pred = true;
            for pred_pc in preds {
                if let Some(&pred_start) = last_pc_to_block_start.get(&pred_pc) {
                    self.block_continuations
                        .entry(pred_start)
                        .or_default()
                        .push((dup_block.start, dup_block.end));

                    // Map duplicated block to first predecessor for dispatch table
                    if first_pred {
                        self.absorbed_to_merged.insert(dup_start, pred_start);
                        first_pred = false;
                    }
                }
            }
        }

        // Remove duplicated blocks
        let new_blocks: Vec<_> = self
            .blocks
            .iter()
            .filter(|b| !to_duplicate.contains(&b.start))
            .cloned()
            .collect();

        let eliminated = self.blocks.len() - new_blocks.len();
        self.blocks = new_blocks;
        eliminated
    }

    /// Form superblocks by absorbing fall-through blocks after branches.
    ///
    /// Returns number of blocks absorbed.
    pub fn form_superblocks(
        &mut self,
        max_depth: usize,
        registry: &ExtensionRegistry<X>,
    ) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }

        let entry_point = self.instruction_table.entry_point();

        // Build lookup map
        let start_to_idx: HashMap<u64, usize> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.start, i))
            .collect();

        // Build set of blocks that are already merge targets
        let merge_targets: HashSet<u64> = self.absorbed_to_merged.values().copied().collect();

        // Find superblock chains
        let mut absorbed = HashSet::new();
        let mut superblock_heads = HashSet::new();
        let mut superblock_chains: HashMap<u64, Vec<u64>> = HashMap::new();

        for block in &self.blocks {
            if absorbed.contains(&block.start) {
                continue;
            }

            // Skip blocks that are merge targets
            if merge_targets.contains(&block.start) {
                continue;
            }

            // Check if block ends with a branch
            let instr = match self.instruction_table.get_at_pc(block.last_pc) {
                Some(i) => i,
                None => continue,
            };

            let ir = registry.lift(instr);
            let (is_branch, taken_pc) = match &ir.terminator {
                rvr_ir::Terminator::Branch { target, .. } => (true, X::to_u64(*target)),
                _ => continue,
            };

            if !is_branch {
                continue;
            }

            // Try to inline the taken path
            if taken_pc != entry_point && start_to_idx.contains_key(&taken_pc) {
                if !absorbed.contains(&taken_pc) && !merge_targets.contains(&taken_pc) {
                    if let Some(preds) = self.predecessors.get(&taken_pc) {
                        if preds.len() == 1 {
                            let taken_idx = start_to_idx[&taken_pc];
                            let taken_block = &self.blocks[taken_idx];
                            if taken_block.instruction_count <= DEFAULT_TAKEN_INLINE_SIZE {
                                // Check if taken block ends with branch
                                if let Some(taken_instr) =
                                    self.instruction_table.get_at_pc(taken_block.last_pc)
                                {
                                    let taken_ir = registry.lift(taken_instr);
                                    if !matches!(taken_ir.terminator, rvr_ir::Terminator::Branch { .. })
                                    {
                                        self.taken_inlines.insert(
                                            block.last_pc,
                                            (taken_block.start, taken_block.end),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Get fall-through target
            let fall_pc = block.end;
            if fall_pc == entry_point || !start_to_idx.contains_key(&fall_pc) {
                continue;
            }

            superblock_heads.insert(block.start);

            // Build superblock chain
            let mut chain = Vec::new();
            let mut current_pc = fall_pc;
            let mut depth = 0;

            while depth < max_depth {
                if absorbed.contains(&current_pc) || current_pc == entry_point {
                    break;
                }
                if !start_to_idx.contains_key(&current_pc) {
                    break;
                }
                if merge_targets.contains(&current_pc) || superblock_heads.contains(&current_pc) {
                    break;
                }

                // Check for multiple predecessors
                if let Some(preds) = self.predecessors.get(&current_pc) {
                    if preds.len() > 1 {
                        break;
                    }
                }

                let current_idx = start_to_idx[&current_pc];
                let current_block = &self.blocks[current_idx];

                // Check terminator
                let term_instr = match self.instruction_table.get_at_pc(current_block.last_pc) {
                    Some(i) => i,
                    None => break,
                };

                let term_ir = registry.lift(term_instr);

                // Absorb this block
                chain.push(current_pc);
                absorbed.insert(current_pc);
                depth += 1;

                // Only continue with FALL/JUMP
                match &term_ir.terminator {
                    rvr_ir::Terminator::Fall { target } => {
                        current_pc = target.map(|t| X::to_u64(t)).unwrap_or(current_block.end);
                    }
                    rvr_ir::Terminator::Jump { target } => {
                        current_pc = X::to_u64(*target);
                    }
                    _ => break,
                }
            }

            if !chain.is_empty() {
                superblock_chains.insert(block.start, chain);
            }
        }

        if absorbed.is_empty() {
            return 0;
        }

        // Update block_continuations and absorbed_to_merged
        for (head_start, chain) in &superblock_chains {
            for &absorbed_start in chain {
                self.absorbed_to_merged.insert(absorbed_start, *head_start);

                let absorbed_idx = start_to_idx[&absorbed_start];
                let absorbed_block = &self.blocks[absorbed_idx];
                self.block_continuations
                    .entry(*head_start)
                    .or_default()
                    .push((absorbed_block.start, absorbed_block.end));
            }
        }

        // Remove absorbed blocks (keep original ends - continuations handle absorbed code)
        let new_blocks: Vec<_> = self
            .blocks
            .iter()
            .filter(|b| !absorbed.contains(&b.start))
            .cloned()
            .collect();

        let absorbed_count = self.blocks.len() - new_blocks.len();
        self.blocks = new_blocks;
        absorbed_count
    }

    /// Apply all transforms in order: merge, tail-dup, superblock.
    pub fn optimize(&mut self, registry: &ExtensionRegistry<X>) -> (usize, usize, usize) {
        let merged = self.merge_blocks(registry);
        let tail_duped = self.tail_duplicate(DEFAULT_TAIL_DUP_SIZE, registry);
        let superblocked = self.form_superblocks(DEFAULT_SUPERBLOCK_DEPTH, registry);

        // Fix any stale mappings from chained absorptions
        self.fix_stale_mappings();

        (merged, tail_duped, superblocked)
    }

    /// Fix stale absorbed_to_merged mappings by following chains.
    ///
    /// After multiple transform passes, a block A might map to block B,
    /// which was subsequently absorbed into block C. This method follows
    /// chains to ensure all mappings point to actually remaining blocks.
    fn fix_stale_mappings(&mut self) {
        // Build set of remaining block starts
        let remaining: HashSet<u64> = self.blocks.iter().map(|b| b.start).collect();

        // For each absorbed block, follow chain to find final target
        let mut to_update = Vec::new();
        let mut to_remove = Vec::new();

        for (&absorbed_pc, &target_pc) in &self.absorbed_to_merged {
            if remaining.contains(&target_pc) {
                // Already points to a remaining block
                continue;
            }

            // Follow chain
            let mut current = target_pc;
            let mut found = false;
            let mut visited = HashSet::new();
            visited.insert(absorbed_pc);

            while !visited.contains(&current) {
                visited.insert(current);

                if remaining.contains(&current) {
                    // Found final target
                    to_update.push((absorbed_pc, current));
                    found = true;
                    break;
                }

                match self.absorbed_to_merged.get(&current) {
                    Some(&next) => current = next,
                    None => break, // Broken chain
                }
            }

            if !found {
                // Broken chain - remove mapping
                to_remove.push(absorbed_pc);
            }
        }

        // Apply updates
        for (pc, target) in to_update {
            self.absorbed_to_merged.insert(pc, target);
        }

        // Remove broken chains
        for pc in to_remove {
            self.absorbed_to_merged.remove(&pc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_block_table_linear() {
        let registry = ExtensionRegistry::<Rv64>::standard();
        // Two ADDI instructions
        let code = [
            0x93, 0x00, 0xa0, 0x02, // addi x1, x0, 42
            0x13, 0x01, 0xb0, 0x03, // addi x2, x0, 59
        ];
        let instr_table = InstructionTable::from_bytes(&code, 0x80000000, &registry);
        let block_table = BlockTable::linear(instr_table);

        assert_eq!(block_table.len(), 2);
        assert_eq!(block_table.blocks[0].start, 0x80000000);
        assert_eq!(block_table.blocks[0].end, 0x80000004);
        assert_eq!(block_table.blocks[1].start, 0x80000004);
        assert_eq!(block_table.blocks[1].end, 0x80000008);
    }

    #[test]
    fn test_block_table_with_branch() {
        let registry = ExtensionRegistry::<Rv64>::standard();
        // BEQ x0, x0, +4 (always taken)
        let code = [
            0x63, 0x02, 0x00, 0x00, // beq x0, x0, 4
            0x13, 0x00, 0x00, 0x00, // nop (unreachable)
            0x93, 0x00, 0xa0, 0x02, // addi x1, x0, 42
        ];
        let instr_table = InstructionTable::from_bytes(&code, 0x80000000, &registry);
        let block_table = BlockTable::from_instruction_table(instr_table, &registry);

        // Should have at least 2 blocks (branch creates leader at target)
        assert!(block_table.len() >= 2);
    }

    #[test]
    fn test_basic_block() {
        let block = BasicBlock::new(0x1000, 0x1010, 4, 0x100c);
        assert_eq!(block.size(), 16);
        assert_eq!(block.instruction_count, 4);
    }
}
