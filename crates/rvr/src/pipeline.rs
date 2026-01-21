//! Recompilation pipeline - ELF → CFG → IR → C.

use std::collections::HashMap;
use std::path::Path;

use rvr_cfg::{BlockTable, InstructionTable};
use rvr_elf::ElfImage;
use rvr_emit::{CProject, EmitConfig, EmitInputs, MemorySegment, NUM_REGS_E, NUM_REGS_I};
use rvr_ir::BlockIR;
use rvr_isa::{ExtensionRegistry, Xlen};
use tracing::{debug, info, info_span};

use crate::{Error, Result};

/// Recompilation pipeline.
pub struct Pipeline<X: Xlen> {
    /// ELF image.
    image: ElfImage<X>,
    /// Emit configuration.
    config: EmitConfig<X>,
    /// Block table (from CFG analysis).
    block_table: Option<BlockTable<X>>,
    /// Lifted IR blocks (keyed by start PC).
    ir_blocks: HashMap<u64, BlockIR<X>>,
    /// Extension registry for decoding and lifting.
    registry: ExtensionRegistry<X>,
}

impl<X: Xlen> Pipeline<X> {
    fn adjust_config_for_image(image: &ElfImage<X>, config: &mut EmitConfig<X>) {
        if image.is_rve() {
            config.num_regs = NUM_REGS_E;
            config.hot_regs.retain(|&r| (r as usize) < NUM_REGS_E);
        } else {
            config.num_regs = NUM_REGS_I;
        }
    }

    /// Create a new pipeline with standard extensions.
    pub fn new(image: ElfImage<X>, config: EmitConfig<X>) -> Self {
        let mut config = config;
        Self::adjust_config_for_image(&image, &mut config);
        Self {
            image,
            config,
            block_table: None,
            ir_blocks: HashMap::new(),
            registry: ExtensionRegistry::standard(),
        }
    }

    /// Create a new pipeline with custom extension registry.
    pub fn with_registry(
        image: ElfImage<X>,
        config: EmitConfig<X>,
        registry: ExtensionRegistry<X>,
    ) -> Self {
        let mut config = config;
        Self::adjust_config_for_image(&image, &mut config);
        Self {
            image,
            config,
            block_table: None,
            ir_blocks: HashMap::new(),
            registry,
        }
    }

    /// Get reference to ELF image.
    pub fn image(&self) -> &ElfImage<X> {
        &self.image
    }

    /// Get reference to emit config.
    pub fn config(&self) -> &EmitConfig<X> {
        &self.config
    }

    /// Get mutable reference to emit config.
    pub fn config_mut(&mut self) -> &mut EmitConfig<X> {
        &mut self.config
    }

    /// Get reference to block table (if built).
    pub fn block_table(&self) -> Option<&BlockTable<X>> {
        self.block_table.as_ref()
    }

    /// Get reference to lifted IR blocks.
    pub fn ir_blocks(&self) -> &HashMap<u64, BlockIR<X>> {
        &self.ir_blocks
    }

    /// Build CFG: creates InstructionTable → BlockTable with optimizations.
    ///
    /// Builds InstructionTable from ALL executable segments, not just the entry segment.
    /// This matches Mojo behavior for multi-segment binaries.
    ///
    /// # Errors
    ///
    /// Returns `Error::NoCodeSegment` if there are no executable segments or
    /// the entry point is not within any executable segment.
    pub fn build_cfg(&mut self) -> Result<()> {
        let _span = info_span!("build_cfg").entered();

        let entry_pc = X::to_u64(self.image.entry_point);

        // Collect all executable segments
        let exec_segments: Vec<_> = self
            .image
            .memory_segments
            .iter()
            .filter(|seg| seg.is_executable())
            .collect();

        if exec_segments.is_empty() {
            return Err(Error::NoCodeSegment(entry_pc));
        }

        // Verify entry point is in an executable segment
        let entry_in_exec = exec_segments.iter().any(|seg| {
            let start = X::to_u64(seg.virtual_start);
            let end = X::to_u64(seg.virtual_end);
            entry_pc >= start && entry_pc < end
        });
        if !entry_in_exec {
            return Err(Error::NoCodeSegment(entry_pc));
        }

        // Calculate address range spanning all executable segments
        let base_address = exec_segments
            .iter()
            .map(|seg| X::to_u64(seg.virtual_start))
            .min()
            .unwrap();
        let end_address = exec_segments
            .iter()
            .map(|seg| X::to_u64(seg.virtual_end))
            .max()
            .unwrap();

        debug!(base_address = format!("{:#x}", base_address), end_address = format!("{:#x}", end_address), "address range");

        // Create InstructionTable spanning all executable segments
        let mut instr_table = InstructionTable::new(base_address, end_address, entry_pc);

        // Populate each executable segment
        for seg in &exec_segments {
            let seg_start = X::to_u64(seg.virtual_start);
            instr_table.populate_segment(&seg.data, seg_start, &self.registry);
        }

        // Add read-only segments for constant propagation
        for seg in &self.image.memory_segments {
            if seg.is_readonly() && !seg.is_executable() {
                let seg_start = X::to_u64(seg.virtual_start);
                let seg_end = X::to_u64(seg.virtual_end);
                instr_table.add_ro_segment(seg_start, seg_end, seg.data.clone());
            }
        }

        let num_instructions = instr_table.valid_indices().count();

        // Create BlockTable with CFG analysis
        let mut block_table = BlockTable::from_instruction_table(instr_table, &self.registry);
        let blocks_before = block_table.len();

        // Apply block transforms (merge, tail-dup, superblock)
        let (absorbed, tail_duplicated, superblocked) = block_table.optimize(&self.registry);

        let num_blocks = block_table.len();
        let insns_per_block = if num_blocks > 0 {
            num_instructions as f64 / num_blocks as f64
        } else {
            0.0
        };

        info!(
            instructions = num_instructions,
            blocks = num_blocks,
            insns_per_block = format!("{:.1}", insns_per_block),
            "built CFG"
        );

        if absorbed > 0 || tail_duplicated > 0 || superblocked > 0 {
            info!(
                before = blocks_before,
                absorbed = absorbed,
                tail_duplicated = tail_duplicated,
                superblocked = superblocked,
                "block transforms"
            );
        }

        self.block_table = Some(block_table);
        Ok(())
    }

    /// Lift all blocks to IR using BlockTable.
    ///
    /// # Errors
    ///
    /// Returns `Error::CfgNotBuilt` if `build_cfg` has not been called.
    pub fn lift_to_ir(&mut self) -> Result<()> {
        let _span = info_span!("lift_to_ir").entered();

        let block_table = self
            .block_table
            .as_ref()
            .ok_or(Error::CfgNotBuilt("lift_to_ir"))?;

        // Collect block info first to avoid borrow issues
        let blocks_info: Vec<_> = block_table.iter().map(|b| (b.start, b.end)).collect();
        let continuations = block_table.block_continuations.clone();

        // Lift each block from BlockTable, following continuations
        for (start, end) in blocks_info {
            let conts = continuations.get(&start);
            if let Some(block_ir) = self.lift_block_with_continuations(start, end, conts) {
                self.ir_blocks.insert(start, block_ir);
            }
        }

        debug!(blocks = self.ir_blocks.len(), "lifted to IR");

        Ok(())
    }

    /// Lift a single block with continuations (absorbed blocks).
    fn lift_block_with_continuations(
        &self,
        start: u64,
        end: u64,
        continuations: Option<&Vec<(u64, u64)>>,
    ) -> Option<BlockIR<X>> {
        let block_table = self.block_table.as_ref()?;
        let instr_table = block_table.instruction_table();

        let mut block = BlockIR::new(X::from_u64(start));

        // Build list of ranges to lift: main block + continuations
        let mut ranges = vec![(start, end)];
        if let Some(conts) = continuations {
            ranges.extend(conts.iter().copied());
        }

        // Lift all ranges
        for (range_idx, (range_start, range_end)) in ranges.iter().enumerate() {
            let is_last_range = range_idx == ranges.len() - 1;
            let mut pc = *range_start;

            while pc < *range_end {
                // Get decoded instruction from table
                let instr = match instr_table.get_at_pc(pc) {
                    Some(i) => i,
                    None => break,
                };

                let size = instr.size as u64;

                // Lift to IR
                let instr_ir = self.registry.lift(instr);

                // Check if this is a control flow terminator
                let is_terminator = instr_ir.terminator.is_control_flow();

                block.push(instr_ir);
                pc += size;

                // Only stop at terminator if this is the LAST range
                // (Terminators in absorbed ranges are internal jumps/falls)
                if is_terminator && is_last_range {
                    break;
                }
            }
        }

        // If block doesn't end with a terminator, add fall-through
        if !block.is_empty() {
            let last_term = &block.instructions.last().unwrap().terminator;
            if !last_term.is_control_flow() {
                // Mark the fall-through target - use end of last range
                let final_pc = ranges.last().map(|(_, e)| *e).unwrap_or(end);
                if let Some(last_instr) = block.instructions.last_mut() {
                    last_instr.terminator = rvr_ir::Terminator::Fall {
                        target: Some(X::from_u64(final_pc)),
                    };
                }
            }
        }

        if block.is_empty() {
            None
        } else {
            Some(block)
        }
    }

    /// Emit C code to output directory using CProject.
    ///
    /// # Errors
    ///
    /// Returns `Error::CfgNotBuilt` if `build_cfg` has not been called.
    /// Returns `Error::Io` if file writing fails.
    pub fn emit_c(&mut self, output_dir: &Path, base_name: &str) -> Result<()> {
        let _span = info_span!("emit_c").entered();

        let block_table = self
            .block_table
            .as_ref()
            .ok_or(Error::CfgNotBuilt("emit_c"))?;

        let entry_point = X::to_u64(self.image.entry_point);

        // Convert memory segments
        let segments: Vec<MemorySegment> = self
            .image
            .memory_segments
            .iter()
            .map(|seg| {
                MemorySegment::new(
                    X::to_u64(seg.virtual_start),
                    seg.data.len(),
                    (X::to_u64(seg.virtual_end) - X::to_u64(seg.virtual_start)) as usize,
                    seg.data.clone(),
                )
            })
            .collect();

        // Compute pc_end from blocks
        let pc_end = self
            .ir_blocks
            .values()
            .map(|b| X::to_u64(b.end_pc))
            .max()
            .unwrap_or(0);

        // Get absorbed_to_merged mapping from BlockTable
        let absorbed_to_merged = block_table.absorbed_to_merged.clone();

        // Get taken_inlines mapping from BlockTable
        let taken_inlines = block_table.taken_inlines.clone();

        // Build derived emission inputs
        let initial_brk = X::to_u64(self.image.get_initial_program_break());
        let mut inputs = EmitInputs::new(entry_point, pc_end).with_initial_brk(initial_brk);
        inputs
            .valid_addresses
            .extend(self.ir_blocks.keys().copied());
        inputs.absorbed_to_merged = absorbed_to_merged.clone();

        // Create CProject with block transform mappings
        let project = CProject::new(output_dir, base_name, self.config.clone())
            .with_inputs(inputs)
            .with_taken_inlines(taken_inlines)
            .with_segments(segments);

        // Collect blocks sorted by start PC
        let mut blocks: Vec<&BlockIR<X>> = self.ir_blocks.values().collect();
        blocks.sort_by_key(|b| X::to_u64(b.start_pc));

        // Clone blocks for write_all (which takes owned)
        let owned_blocks: Vec<BlockIR<X>> = blocks.into_iter().cloned().collect();

        // Write all files
        project.write_all(&owned_blocks)?;

        Ok(())
    }

    /// Get statistics.
    pub fn stats(&self) -> PipelineStats {
        let block_table = self.block_table.as_ref();
        PipelineStats {
            num_blocks: self.ir_blocks.len(),
            num_basic_blocks: block_table.map(|b| b.len()).unwrap_or(0),
            num_absorbed: block_table.map(|b| b.absorbed_to_merged.len()).unwrap_or(0),
        }
    }
}

/// Pipeline statistics.
#[derive(Debug, Default)]
pub struct PipelineStats {
    /// Number of lifted IR blocks.
    pub num_blocks: usize,
    /// Number of basic blocks from CFG analysis.
    pub num_basic_blocks: usize,
    /// Number of blocks absorbed (merged/tail-duped).
    pub num_absorbed: usize,
}
