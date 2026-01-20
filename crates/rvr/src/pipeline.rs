//! Recompilation pipeline - ELF → CFG → IR → C.

use std::collections::HashMap;
use std::path::Path;

use rvr_cfg::{CfgAnalyzer, CfgResult};
use rvr_elf::ElfImage;
use rvr_emit::{CProject, EmitConfig, MemorySegment};
use rvr_ir::BlockIR;
use rvr_isa::{Xlen, CompositeDecoder, InstructionExtension};

use crate::Result;

/// Recompilation pipeline.
pub struct Pipeline<X: Xlen> {
    /// ELF image.
    pub image: ElfImage<X>,
    /// Emit configuration.
    pub config: EmitConfig<X>,
    /// CFG analysis result.
    pub cfg_result: Option<CfgResult>,
    /// Lifted IR blocks (keyed by start PC).
    pub ir_blocks: HashMap<u64, BlockIR<X>>,
    /// Instruction decoder (supports custom extensions).
    pub decoder: CompositeDecoder<X>,
}

impl<X: Xlen> Pipeline<X> {
    /// Create a new pipeline with default decoder.
    pub fn new(image: ElfImage<X>, config: EmitConfig<X>) -> Self {
        Self {
            image,
            config,
            cfg_result: None,
            ir_blocks: HashMap::new(),
            decoder: CompositeDecoder::default(),
        }
    }

    /// Create a new pipeline with custom extensions.
    pub fn with_extensions(
        image: ElfImage<X>,
        config: EmitConfig<X>,
        extensions: Vec<Box<dyn InstructionExtension<X>>>,
    ) -> Self {
        Self {
            image,
            config,
            cfg_result: None,
            ir_blocks: HashMap::new(),
            decoder: CompositeDecoder::new(extensions),
        }
    }

    /// Add an extension to the decoder chain.
    pub fn add_extension(&mut self, ext: impl InstructionExtension<X> + 'static) {
        self.decoder = std::mem::take(&mut self.decoder).with_extension(ext);
    }

    /// Run CFG analysis.
    pub fn analyze_cfg(&mut self) {
        let analyzer = CfgAnalyzer::<X>::new(&self.image);
        self.cfg_result = Some(analyzer.analyze());
    }

    /// Lift all blocks to IR.
    pub fn lift_to_ir(&mut self) {
        let cfg = self.cfg_result.as_ref().expect("CFG analysis must be run first");

        // Get sorted leaders
        let mut leaders: Vec<u64> = cfg.leaders.iter().copied().collect();
        leaders.sort_unstable();

        // For each leader, find the block extent and lift instructions
        for (i, &leader) in leaders.iter().enumerate() {
            // Find block end (next leader or end of code)
            let block_end = if i + 1 < leaders.len() {
                leaders[i + 1]
            } else {
                // Find the end from successors or use a reasonable default
                self.find_block_end(leader, cfg)
            };

            // Lift the block
            if let Some(block_ir) = self.lift_block(leader, block_end) {
                self.ir_blocks.insert(leader, block_ir);
            }
        }

        // Update config with valid addresses
        for &addr in self.ir_blocks.keys() {
            self.config.valid_addresses.insert(addr);
        }
    }

    /// Find block end address.
    fn find_block_end(&self, start: u64, cfg: &CfgResult) -> u64 {
        // Use successors to find where the block ends
        if let Some(succs) = cfg.successors.get(&start) {
            if !succs.is_empty() {
                // Block ends at the instruction that branches
                return start + 4; // Conservative: assume single instruction block
            }
        }
        start + 4
    }

    /// Lift a single block from start to end.
    fn lift_block(&self, start: u64, end: u64) -> Option<BlockIR<X>> {
        let mut block = BlockIR::new(X::from_u64(start));

        let mut pc = start;
        while pc < end {
            // Read instruction bytes
            let Some((raw, size)) = self.read_instr(pc) else {
                break;
            };

            // Decode using CompositeDecoder (supports custom extensions)
            let bytes = raw.to_le_bytes();
            let decoded = match self.decoder.decode(&bytes, X::from_u64(pc)) {
                Some(d) => d,
                None => break,
            };

            // Lift to IR using CompositeDecoder
            let instr_ir = self.decoder.lift(&decoded);

            // Check if this instruction ends the block
            let is_terminator = instr_ir.terminator.is_control_flow();

            block.push(instr_ir);

            pc += size as u64;

            if is_terminator {
                break;
            }
        }

        if block.is_empty() {
            None
        } else {
            Some(block)
        }
    }

    /// Read instruction bytes at PC.
    fn read_instr(&self, pc: u64) -> Option<(u32, u8)> {
        for seg in &self.image.memory_segments {
            let start = X::to_u64(seg.virtual_start);
            let end = X::to_u64(seg.virtual_end);
            if pc >= start && pc < end {
                let offset = (pc - start) as usize;
                if offset >= seg.data.len() {
                    return None;
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

    /// Emit C code to output directory using CProject.
    pub fn emit_c(&mut self, output_dir: &Path, base_name: &str) -> Result<()> {
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

        // Create CProject
        let project = CProject::new(output_dir, base_name, self.config.clone())
            .with_entry_point(X::to_u64(self.image.entry_point))
            .with_pc_end(pc_end)
            .with_valid_addresses(self.config.valid_addresses.clone())
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
        let cfg = self.cfg_result.as_ref();
        PipelineStats {
            num_blocks: self.ir_blocks.len(),
            num_leaders: cfg.map(|c| c.leaders.len()).unwrap_or(0),
            num_functions: cfg.map(|c| c.function_entries.len()).unwrap_or(0),
        }
    }
}

/// Pipeline statistics.
#[derive(Debug, Default)]
pub struct PipelineStats {
    pub num_blocks: usize,
    pub num_leaders: usize,
    pub num_functions: usize,
}
