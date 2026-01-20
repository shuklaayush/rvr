//! Recompilation pipeline - ELF → CFG → IR → C.

use std::collections::HashMap;
use std::path::Path;

use rvr_cfg::{BlockTable, InstructionTable};
use rvr_elf::ElfImage;
use rvr_emit::{CProject, EmitConfig, MemorySegment};
use rvr_ir::BlockIR;
use rvr_isa::{ExtensionRegistry, Xlen};

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
    /// Create a new pipeline with standard extensions.
    pub fn new(image: ElfImage<X>, config: EmitConfig<X>) -> Self {
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
    /// # Errors
    ///
    /// Returns `Error::NoCodeSegment` if the entry point is not within any memory segment.
    pub fn build_cfg(&mut self) -> Result<()> {
        // Find the code segment containing entry point
        let entry_pc = X::to_u64(self.image.entry_point);
        let (code_start, _code_end, code_data) = self.find_code_segment(entry_pc)
            .ok_or(Error::NoCodeSegment(entry_pc))?;

        // Create InstructionTable from code segment
        let mut instr_table = InstructionTable::from_bytes(
            &code_data,
            code_start,
            &self.registry,
        );
        instr_table.set_entry_point(entry_pc);

        // Add read-only segments for constant propagation
        for seg in &self.image.memory_segments {
            let seg_start = X::to_u64(seg.virtual_start);
            let seg_end = X::to_u64(seg.virtual_end);
            if seg_start != code_start {
                instr_table.add_ro_segment(seg_start, seg_end, seg.data.clone());
            }
        }

        // Create BlockTable with CFG analysis
        let mut block_table = BlockTable::from_instruction_table(instr_table, &self.registry);

        // Apply block transforms (merge, tail-dup, superblock)
        block_table.optimize(&self.registry);

        self.block_table = Some(block_table);
        Ok(())
    }

    /// Find code segment containing the given PC.
    fn find_code_segment(&self, pc: u64) -> Option<(u64, u64, Vec<u8>)> {
        for seg in &self.image.memory_segments {
            let start = X::to_u64(seg.virtual_start);
            let end = X::to_u64(seg.virtual_end);
            if pc >= start && pc < end {
                return Some((start, end, seg.data.clone()));
            }
        }
        None
    }

    /// Lift all blocks to IR using BlockTable.
    ///
    /// # Errors
    ///
    /// Returns `Error::CfgNotBuilt` if `build_cfg` has not been called.
    pub fn lift_to_ir(&mut self) -> Result<()> {
        let block_table = self.block_table.as_ref()
            .ok_or(Error::CfgNotBuilt("lift_to_ir"))?;

        // Collect block info first to avoid borrow issues
        let blocks_info: Vec<_> = block_table.iter()
            .map(|b| (b.start, b.end))
            .collect();
        let continuations = block_table.block_continuations.clone();

        // Lift each block from BlockTable, following continuations
        for (start, end) in blocks_info {
            let conts = continuations.get(&start);
            if let Some(block_ir) = self.lift_block_with_continuations(start, end, conts) {
                self.ir_blocks.insert(start, block_ir);
            }
        }

        // Update config with valid addresses from blocks
        // NOTE: Only add actual block addresses, NOT absorbed addresses.
        // Absorbed addresses are handled separately via absorbed_to_merged mapping.
        for &addr in self.ir_blocks.keys() {
            self.config.valid_addresses.insert(addr);
        }

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

    /// Lift a single block from start to end (legacy, without continuations).
    #[allow(dead_code)]
    fn lift_block(&self, start: u64, end: u64) -> Option<BlockIR<X>> {
        self.lift_block_with_continuations(start, end, None)
    }

    /// Emit C code to output directory using CProject.
    ///
    /// # Errors
    ///
    /// Returns `Error::CfgNotBuilt` if `build_cfg` has not been called.
    /// Returns `Error::Io` if file writing fails.
    pub fn emit_c(&mut self, output_dir: &Path, base_name: &str) -> Result<()> {
        let block_table = self.block_table.as_ref()
            .ok_or(Error::CfgNotBuilt("emit_c"))?;

        // Set entry point in config
        self.config.entry_point = self.image.entry_point;

        // Collect valid addresses
        for &addr in self.ir_blocks.keys() {
            self.config.valid_addresses.insert(addr);
        }

        // Convert memory segments
        let segments: Vec<MemorySegment> = self.image.memory_segments
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
        let pc_end = self.ir_blocks.values()
            .map(|b| X::to_u64(b.end_pc))
            .max()
            .unwrap_or(0);

        // Get absorbed_to_merged mapping from BlockTable
        let absorbed_to_merged = block_table.absorbed_to_merged.clone();

        // Add absorbed mapping to config for emitter use
        self.config.absorbed_to_merged = absorbed_to_merged.clone();

        // Create CProject with block transform mappings
        let project = CProject::new(output_dir, base_name, self.config.clone())
            .with_entry_point(X::to_u64(self.image.entry_point))
            .with_pc_end(pc_end)
            .with_valid_addresses(self.config.valid_addresses.clone())
            .with_absorbed_mapping(absorbed_to_merged)
            .with_segments(segments)
            .with_tohost(self.config.tohost_enabled);

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
