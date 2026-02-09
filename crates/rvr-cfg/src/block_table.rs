//! Block table for CFG analysis and block transforms.
//!
//! Supports merge, tail-dup, and superblock transforms.

use std::collections::HashMap;

use rustc_hash::{FxHashMap, FxHashSet};
use rvr_isa::{ExtensionRegistry, Xlen};
use tracing::{debug, trace, trace_span};

use crate::InstructionTable;
use crate::analysis::ControlFlowAnalyzer;

// TODO: why both end and last_pc - maybe should have terminator type field
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
    #[must_use]
    pub const fn new(start: u64, end: u64, instruction_count: usize, last_pc: u64) -> Self {
        Self {
            start,
            end,
            instruction_count,
            last_pc,
        }
    }

    // TODO: better name that clarifies that this is bytes
    /// Size of block in bytes.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.end - self.start
    }
}

// TODO: seems like there's redundancy here
/// Block table with CFG analysis and transforms.
pub struct BlockTable<X: Xlen> {
    /// List of basic blocks.
    pub blocks: Vec<BasicBlock>,
    // TODO: use fxhashmap
    /// Absorbed PC -> merged block start mapping (for dispatch table).
    pub absorbed_to_merged: HashMap<u64, u64>,
    /// Block continuations: `merged_start` -> list of (start, end) ranges.
    pub block_continuations: HashMap<u64, Vec<(u64, u64)>>,
    /// Taken path inlines: `branch_pc` -> (`inline_start`, `inline_end`).
    pub taken_inlines: HashMap<u64, (u64, u64)>,
    /// Predecessors map: PC -> set of predecessor PCs.
    pub predecessors: FxHashMap<u64, FxHashSet<u64>>,
    /// Successors map: PC -> set of successor PCs.
    pub successors: FxHashMap<u64, FxHashSet<u64>>,
    /// Unresolved dynamic jumps.
    pub unresolved_jumps: FxHashSet<u64>,
    /// Call return map: callee -> set of return addresses.
    pub call_return_map: FxHashMap<u64, FxHashSet<u64>>,
    /// Block to function mapping: `block_start` -> `function_entry`.
    pub block_to_function: FxHashMap<u64, u64>,
    /// Reference to instruction table.
    instruction_table: InstructionTable<X>,
}

// TODO: superblock stuff should be encapsulated
/// Default limits for block transforms.
pub const DEFAULT_SUPERBLOCK_DEPTH: usize = 100;
pub const DEFAULT_TAIL_DUP_SIZE: usize = 100;
pub const DEFAULT_TAKEN_INLINE_SIZE: usize = 50;

type SuperblockPlan = (
    FxHashSet<u64>,
    HashMap<u64, Vec<u64>>,
    Vec<(u64, (u64, u64))>,
);

struct SuperblockContext<'a, X: Xlen> {
    entry_points: &'a [u64],
    start_to_idx: &'a HashMap<u64, usize>,
    merge_targets: &'a FxHashSet<u64>,
    registry: &'a ExtensionRegistry<X>,
}

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
            predecessors: FxHashMap::default(),
            successors: FxHashMap::default(),
            unresolved_jumps: FxHashSet::default(),
            call_return_map: FxHashMap::default(),
            block_to_function: FxHashMap::default(),
            instruction_table,
        };
        table.build_blocks(registry);
        debug!(
            blocks = table.blocks.len(),
            unresolved_jumps = table.unresolved_jumps.len(),
            "built block table"
        );
        table
    }

    /// Create a block table with linear blocks (one instruction per block).
    #[must_use]
    pub fn linear(instruction_table: InstructionTable<X>) -> Self {
        let mut table = Self {
            blocks: Vec::new(),
            absorbed_to_merged: HashMap::new(),
            block_continuations: HashMap::new(),
            taken_inlines: HashMap::new(),
            predecessors: FxHashMap::default(),
            successors: FxHashMap::default(),
            unresolved_jumps: FxHashSet::default(),
            call_return_map: FxHashMap::default(),
            block_to_function: FxHashMap::default(),
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
            let size = u64::from(self.instruction_table.instruction_size_at_pc(pc));
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
        let analysis = ControlFlowAnalyzer::analyze(&self.instruction_table);

        self.predecessors = analysis.predecessors;
        self.successors = analysis.successors;
        self.unresolved_jumps = analysis.unresolved_dynamic_jumps;
        self.call_return_map = analysis.call_return_map;
        self.block_to_function = analysis.block_to_function;

        {
            let _span = trace_span!("create_blocks").entered();
            self.create_blocks_from_leaders(&analysis.leaders, registry);
        }
    }

    /// Create blocks from leader set.
    fn create_blocks_from_leaders(
        &mut self,
        leaders: &FxHashSet<u64>,
        registry: &ExtensionRegistry<X>,
    ) {
        // Sort leaders
        // TODO: why sort everywere, maybe just store sorted everywhere
        let mut sorted_leaders: Vec<u64> = leaders.iter().copied().collect();
        sorted_leaders.sort_unstable();

        let end = self.instruction_table.end_address();

        // TODO: more idiomatic
        for (i, &block_start) in sorted_leaders.iter().enumerate() {
            if !self.instruction_table.is_valid_pc(block_start) {
                continue;
            }

            // Find max end PC (next leader or table end)
            let max_end = sorted_leaders.get(i + 1).copied().unwrap_or(end).min(end);

            let mut pc = block_start;
            let mut instruction_count = 0;
            let mut last_pc = block_start;

            while pc < max_end && pc < end {
                if !self.instruction_table.is_valid_pc(pc) {
                    break;
                }

                let size = u64::from(self.instruction_table.instruction_size_at_pc(pc));
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
                self.blocks
                    .push(BasicBlock::new(block_start, pc, instruction_count, last_pc));
            }
        }
    }

    /// Get instruction table reference.
    #[must_use]
    pub const fn instruction_table(&self) -> &InstructionTable<X> {
        &self.instruction_table
    }

    /// Get number of blocks.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.blocks.len()
    }

    // TODO: can i use some trait for this
    /// Check if empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    // TODO: can i use some trait for this
    /// Get block by index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&BasicBlock> {
        self.blocks.get(index)
    }

    // TODO: can i use some trait for this
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

        let entry_points: Vec<u64> = self.instruction_table.entry_points().to_vec();

        // Build lookup map
        let start_to_idx: HashMap<u64, usize> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.start, i))
            .collect();

        // Find absorbable blocks
        let mut absorbed = FxHashSet::default();
        for block in &self.blocks {
            if absorbed.contains(&block.start) {
                continue;
            }
            if let Some(target) = self.get_merge_target(block, &entry_points, registry) {
                absorbed.insert(target);
            }
        }

        if absorbed.is_empty() {
            return 0;
        }

        // Build merged blocks with continuation chains
        let mut merged = Vec::new();
        // TODO: maybe shouldn't be in state if being cleared - doesn't seem idiomatic
        self.absorbed_to_merged.clear();
        self.block_continuations.clear();

        // TODO: doesn't seem idiomatic - think in abstract that this should be some recursive algorithm to keep on merging
        //       static jump targets or maybe this is something else and should be handled separate from general merging
        // TODO: what is continuations
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
                let target = self.get_merge_target(&current_block, &entry_points, registry);
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
                self.block_continuations
                    .insert(block.start, continuations.clone());
            }

            // Keep original end - continuations handle absorbed blocks
            merged.push(BasicBlock::new(block.start, block.end, count, last_pc));
        }

        let absorbed_count = self.blocks.len() - merged.len();
        self.blocks = merged;
        if absorbed_count > 0 {
            trace!(absorbed = absorbed_count, "merge_blocks complete");
        }
        absorbed_count
    }

    /// Get merge target if block can merge with its successor.
    fn get_merge_target(
        &self,
        block: &BasicBlock,
        entry_points: &[u64],
        registry: &ExtensionRegistry<X>,
    ) -> Option<u64> {
        let instr = self.instruction_table.get_at_pc(block.last_pc)?;
        let ir = registry.lift(instr);

        // TODO: maybe can be encapsulated into something
        let target_pc = match &ir.terminator {
            rvr_ir::Terminator::Fall { target } => target.map(|t| X::to_u64(t))?,
            rvr_ir::Terminator::Jump { target } => X::to_u64(*target),
            _ => return None,
        };

        // Don't merge into entry point blocks
        if entry_points.contains(&target_pc) {
            return None;
        }

        // Must have exactly one predecessor (this block's last instruction)
        let preds = self.predecessors.get(&target_pc)?;
        if preds.len() != 1 || !preds.contains(&block.last_pc) {
            return None;
        }

        Some(target_pc)
    }

    // TODO: see if can be simplified - maybe should be separate block type
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

        let entry_points = self.instruction_table.entry_points();
        let start_to_idx: HashMap<u64, usize> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.start, i))
            .collect();
        let last_pc_to_block_start: HashMap<u64, u64> =
            self.blocks.iter().map(|b| (b.last_pc, b.start)).collect();

        let to_duplicate =
            self.collect_tail_duplicate_candidates(entry_points, max_dup_size, registry);
        if to_duplicate.is_empty() {
            return 0;
        }

        for &dup_start in &to_duplicate {
            let Some(&dup_idx) = start_to_idx.get(&dup_start) else {
                continue;
            };
            let dup_range = {
                let dup_block = &self.blocks[dup_idx];
                (dup_block.start, dup_block.end)
            };
            let Some(preds) = self.predecessors.get(&dup_start) else {
                continue;
            };

            let valid_preds =
                self.collect_tail_duplicate_predecessors(preds, &last_pc_to_block_start, registry);
            if valid_preds.is_empty() {
                continue;
            }
            self.apply_tail_duplication(dup_start, dup_range, &valid_preds);
        }

        let new_blocks: Vec<_> = self
            .blocks
            .iter()
            .filter(|b| !to_duplicate.contains(&b.start))
            .cloned()
            .collect();

        let eliminated = self.blocks.len() - new_blocks.len();
        self.blocks = new_blocks;
        if eliminated > 0 {
            trace!(eliminated = eliminated, "tail_duplicate complete");
        }
        eliminated
    }

    fn collect_tail_duplicate_candidates(
        &self,
        entry_points: &[u64],
        max_dup_size: usize,
        registry: &ExtensionRegistry<X>,
    ) -> FxHashSet<u64> {
        let mut to_duplicate = FxHashSet::default();

        for block in &self.blocks {
            if entry_points.contains(&block.start) {
                continue;
            }
            if block.instruction_count > max_dup_size {
                continue;
            }

            let Some(preds) = self.predecessors.get(&block.start) else {
                continue;
            };
            if preds.len() < 2 {
                continue;
            }

            if !self.block_ends_with_fall(block, registry) {
                continue;
            }

            if preds
                .iter()
                .all(|&pred_pc| self.pred_is_unconditional_jump(pred_pc, registry))
            {
                to_duplicate.insert(block.start);
            }
        }

        to_duplicate
    }

    fn block_ends_with_fall(&self, block: &BasicBlock, registry: &ExtensionRegistry<X>) -> bool {
        self.instruction_table
            .get_at_pc(block.last_pc)
            .is_some_and(|instr| {
                matches!(
                    registry.lift(instr).terminator,
                    rvr_ir::Terminator::Fall { .. }
                )
            })
    }

    fn pred_is_unconditional_jump(&self, pred_pc: u64, registry: &ExtensionRegistry<X>) -> bool {
        self.instruction_table
            .get_at_pc(pred_pc)
            .is_some_and(|instr| {
                let ir = registry.lift(instr);
                matches!(ir.terminator, rvr_ir::Terminator::Jump { .. })
            })
    }

    fn collect_tail_duplicate_predecessors(
        &self,
        preds: &FxHashSet<u64>,
        last_pc_to_block_start: &HashMap<u64, u64>,
        registry: &ExtensionRegistry<X>,
    ) -> Vec<u64> {
        let mut valid_preds: Vec<(u64, bool)> = preds
            .iter()
            .filter_map(|&pred_pc| {
                let pred_start = *last_pc_to_block_start.get(&pred_pc)?;
                let instr = self.instruction_table.get_at_pc(pred_pc)?;
                let ir = registry.lift(instr);
                let (is_direct, is_explicit) = match &ir.terminator {
                    rvr_ir::Terminator::Jump { .. } | rvr_ir::Terminator::Branch { .. } => {
                        (true, true)
                    }
                    rvr_ir::Terminator::Fall { .. } => (true, false),
                    _ => (false, false),
                };
                if is_direct {
                    Some((pred_start, is_explicit))
                } else {
                    None
                }
            })
            .collect();

        valid_preds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        valid_preds.into_iter().map(|(addr, _)| addr).collect()
    }

    fn apply_tail_duplication(
        &mut self,
        dup_start: u64,
        dup_range: (u64, u64),
        valid_preds: &[u64],
    ) {
        let mut first_pred = true;
        for &pred_start in valid_preds {
            self.block_continuations
                .entry(pred_start)
                .or_default()
                .push(dup_range);

            if first_pred {
                self.absorbed_to_merged.insert(dup_start, pred_start);
                first_pred = false;
            }
        }
    }

    /// Form superblocks by absorbing fall-through blocks after branches.
    ///
    /// Returns number of blocks absorbed.
    pub fn form_superblocks(&mut self, max_depth: usize, registry: &ExtensionRegistry<X>) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }

        let entry_points = self.instruction_table.entry_points();
        let start_to_idx: HashMap<u64, usize> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.start, i))
            .collect();
        let merge_targets: FxHashSet<u64> = self.absorbed_to_merged.values().copied().collect();

        let (absorbed, superblock_chains, pending_inlines) = self.collect_superblock_chains(
            entry_points,
            &start_to_idx,
            &merge_targets,
            max_depth,
            registry,
        );

        for (pc, range) in pending_inlines {
            self.taken_inlines.insert(pc, range);
        }

        if absorbed.is_empty() {
            return 0;
        }

        self.apply_superblock_chains(&superblock_chains, &start_to_idx);

        let new_blocks: Vec<_> = self
            .blocks
            .iter()
            .filter(|b| !absorbed.contains(&b.start))
            .cloned()
            .collect();

        let absorbed_count = self.blocks.len() - new_blocks.len();
        self.blocks = new_blocks;
        if absorbed_count > 0 {
            trace!(
                absorbed = absorbed_count,
                taken_inlines = self.taken_inlines.len(),
                "form_superblocks complete"
            );
        }
        absorbed_count
    }

    fn collect_superblock_chains(
        &self,
        entry_points: &[u64],
        start_to_idx: &HashMap<u64, usize>,
        merge_targets: &FxHashSet<u64>,
        max_depth: usize,
        registry: &ExtensionRegistry<X>,
    ) -> SuperblockPlan {
        let mut absorbed = FxHashSet::default();
        let mut superblock_heads = FxHashSet::default();
        let mut superblock_chains: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut pending_inlines = Vec::new();
        let context = SuperblockContext {
            entry_points,
            start_to_idx,
            merge_targets,
            registry,
        };

        for block in &self.blocks {
            if absorbed.contains(&block.start) || merge_targets.contains(&block.start) {
                continue;
            }

            let Some(instr) = self.instruction_table.get_at_pc(block.last_pc) else {
                continue;
            };
            let ir = registry.lift(instr);
            let rvr_ir::Terminator::Branch { target, .. } = &ir.terminator else {
                continue;
            };
            let taken_pc = X::to_u64(*target);

            if let Some(inline) = self.maybe_inline_taken_path(block, taken_pc, &absorbed, &context)
            {
                pending_inlines.push(inline);
            }

            let fall_pc = block.end;
            if entry_points.contains(&fall_pc) || !start_to_idx.contains_key(&fall_pc) {
                continue;
            }

            superblock_heads.insert(block.start);

            let chain = self.build_superblock_chain(
                fall_pc,
                &superblock_heads,
                &mut absorbed,
                &context,
                max_depth,
            );

            if !chain.is_empty() {
                superblock_chains.insert(block.start, chain);
            }
        }

        (absorbed, superblock_chains, pending_inlines)
    }

    fn maybe_inline_taken_path(
        &self,
        block: &BasicBlock,
        taken_pc: u64,
        absorbed: &FxHashSet<u64>,
        context: &SuperblockContext<'_, X>,
    ) -> Option<(u64, (u64, u64))> {
        if context.entry_points.contains(&taken_pc)
            || !context.start_to_idx.contains_key(&taken_pc)
            || absorbed.contains(&taken_pc)
            || context.merge_targets.contains(&taken_pc)
        {
            return None;
        }

        let preds = self.predecessors.get(&taken_pc)?;
        if preds.len() != 1 {
            return None;
        }

        let taken_idx = context.start_to_idx[&taken_pc];
        let taken_block = &self.blocks[taken_idx];
        if taken_block.instruction_count > DEFAULT_TAKEN_INLINE_SIZE {
            return None;
        }

        let taken_instr = self.instruction_table.get_at_pc(taken_block.last_pc)?;
        let taken_ir = context.registry.lift(taken_instr);
        if matches!(taken_ir.terminator, rvr_ir::Terminator::Branch { .. }) {
            return None;
        }

        Some((block.last_pc, (taken_block.start, taken_block.end)))
    }

    fn build_superblock_chain(
        &self,
        mut current_pc: u64,
        superblock_heads: &FxHashSet<u64>,
        absorbed: &mut FxHashSet<u64>,
        context: &SuperblockContext<'_, X>,
        max_depth: usize,
    ) -> Vec<u64> {
        let mut chain = Vec::new();
        let mut depth = 0;

        while depth < max_depth {
            if absorbed.contains(&current_pc) || context.entry_points.contains(&current_pc) {
                break;
            }
            if !context.start_to_idx.contains_key(&current_pc) {
                break;
            }
            if context.merge_targets.contains(&current_pc) || superblock_heads.contains(&current_pc)
            {
                break;
            }
            if self
                .predecessors
                .get(&current_pc)
                .is_some_and(|preds| preds.len() > 1)
            {
                break;
            }

            let current_idx = context.start_to_idx[&current_pc];
            let current_block = &self.blocks[current_idx];
            let Some(term_instr) = self.instruction_table.get_at_pc(current_block.last_pc) else {
                break;
            };
            let term_ir = context.registry.lift(term_instr);

            chain.push(current_pc);
            absorbed.insert(current_pc);
            depth += 1;

            match &term_ir.terminator {
                rvr_ir::Terminator::Fall { target } => {
                    current_pc = target.map_or(current_block.end, |t| X::to_u64(t));
                }
                rvr_ir::Terminator::Jump { target } => {
                    current_pc = X::to_u64(*target);
                }
                _ => break,
            }
        }

        chain
    }

    fn apply_superblock_chains(
        &mut self,
        superblock_chains: &HashMap<u64, Vec<u64>>,
        start_to_idx: &HashMap<u64, usize>,
    ) {
        for (head_start, chain) in superblock_chains {
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
    }

    /// Apply all transforms in order: merge, tail-dup, superblock.
    pub fn optimize(&mut self, registry: &ExtensionRegistry<X>) -> (usize, usize, usize) {
        let merged = {
            let _span = trace_span!("merge_blocks").entered();
            self.merge_blocks(registry)
        };
        // TODO: both of these are similar and there should be a generic way to do this
        let tail_duped = {
            let _span = trace_span!("tail_duplicate").entered();
            self.tail_duplicate(DEFAULT_TAIL_DUP_SIZE, registry)
        };
        let superblocked = {
            let _span = trace_span!("form_superblocks").entered();
            self.form_superblocks(DEFAULT_SUPERBLOCK_DEPTH, registry)
        };

        // Fix any stale mappings from chained absorptions
        // TODO: this shouldn't be a separate function and should happen above
        self.fix_stale_mappings();

        (merged, tail_duped, superblocked)
    }

    /// Fix stale `absorbed_to_merged` mappings by following chains.
    ///
    /// After multiple transform passes, a block A might map to block B,
    /// which was subsequently absorbed into block C. This method follows
    /// chains to ensure all mappings point to actually remaining blocks.
    pub fn fix_stale_mappings(&mut self) {
        // Build set of remaining block starts
        let remaining: FxHashSet<u64> = self.blocks.iter().map(|b| b.start).collect();

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
            let mut visited = FxHashSet::default();
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

            let mut moved_range = None;
            for ranges in self.block_continuations.values_mut() {
                if let Some(pos) = ranges.iter().position(|(start, _)| *start == pc) {
                    moved_range = Some(ranges.remove(pos));
                    break;
                }
            }

            if let Some(range) = moved_range {
                self.block_continuations
                    .entry(target)
                    .or_default()
                    .push(range);
            }
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
        let instr_table = InstructionTable::from_bytes(&code, 0x8000_0000, &registry);
        let block_table = BlockTable::linear(instr_table);

        assert_eq!(block_table.len(), 2);
        assert_eq!(block_table.blocks[0].start, 0x8000_0000);
        assert_eq!(block_table.blocks[0].end, 0x8000_0004);
        assert_eq!(block_table.blocks[1].start, 0x8000_0004);
        assert_eq!(block_table.blocks[1].end, 0x8000_0008);
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
        let instr_table = InstructionTable::from_bytes(&code, 0x8000_0000, &registry);
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
