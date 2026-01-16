//! Recompilation pipeline - ELF → CFG → IR → C.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use rvr_cfg::{CfgAnalyzer, CfgResult};
use rvr_elf::ElfImage;
use rvr_emit::{CEmitter, EmitConfig};
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

    /// Emit C code to output directory.
    pub fn emit_c(&mut self, output_dir: &Path, base_name: &str) -> Result<()> {
        // Set entry point in config
        self.config.entry_point = self.image.entry_point;

        // Create emitter
        let mut emitter = CEmitter::<X>::new(self.config.clone());

        // Emit header file
        self.emit_header(output_dir, base_name)?;

        // Emit partition files
        self.emit_partitions(output_dir, base_name, &mut emitter)?;

        // Emit dispatch table
        self.emit_dispatch(output_dir, base_name)?;

        // Emit Makefile
        self.emit_makefile(output_dir, base_name)?;

        Ok(())
    }

    /// Emit header file.
    fn emit_header(&self, output_dir: &Path, base_name: &str) -> Result<()> {
        let path = output_dir.join(format!("{}.h", base_name));
        let mut file = std::fs::File::create(&path)?;

        let reg_type = if X::VALUE == 64 { "uint64_t" } else { "uint32_t" };
        let addr_type = if X::VALUE == 64 { "uint64_t" } else { "uint32_t" };

        writeln!(file, "// Generated by RVR - RISC-V Recompiler")?;
        writeln!(file, "#pragma once")?;
        writeln!(file)?;
        writeln!(file, "#include <stdint.h>")?;
        writeln!(file, "#include <stdbool.h>")?;
        writeln!(file)?;
        writeln!(file, "#define XLEN {}", X::VALUE)?;
        writeln!(file, "#define likely(x) __builtin_expect(!!(x), 1)")?;
        writeln!(file, "#define unlikely(x) __builtin_expect(!!(x), 0)")?;
        writeln!(file)?;
        writeln!(file, "// VM state")?;
        writeln!(file, "typedef struct RvState {{")?;
        writeln!(file, "    {} pc;", addr_type)?;
        writeln!(file, "    {} regs[32];", reg_type)?;
        writeln!(file, "    uint64_t cycle;")?;
        writeln!(file, "    uint64_t instret;")?;
        writeln!(file, "    bool has_exited;")?;
        writeln!(file, "    uint8_t exit_code;")?;
        writeln!(file, "}} RvState;")?;
        writeln!(file)?;
        writeln!(file, "// Block function type")?;
        writeln!(file, "typedef void (*BlockFn)(RvState* state, uint8_t* memory, {} instret, {}* regs);", reg_type, reg_type)?;
        writeln!(file)?;
        writeln!(file, "// Memory access functions")?;
        writeln!(file, "static inline int8_t rd_mem_i8(uint8_t* mem, {} addr, int offset) {{", addr_type)?;
        writeln!(file, "    return *(int8_t*)(mem + addr + offset);")?;
        writeln!(file, "}}")?;
        writeln!(file, "static inline uint8_t rd_mem_u8(uint8_t* mem, {} addr, int offset) {{", addr_type)?;
        writeln!(file, "    return *(uint8_t*)(mem + addr + offset);")?;
        writeln!(file, "}}")?;
        writeln!(file, "static inline int16_t rd_mem_i16(uint8_t* mem, {} addr, int offset) {{", addr_type)?;
        writeln!(file, "    return *(int16_t*)(mem + addr + offset);")?;
        writeln!(file, "}}")?;
        writeln!(file, "static inline uint16_t rd_mem_u16(uint8_t* mem, {} addr, int offset) {{", addr_type)?;
        writeln!(file, "    return *(uint16_t*)(mem + addr + offset);")?;
        writeln!(file, "}}")?;
        writeln!(file, "static inline int32_t rd_mem_i32(uint8_t* mem, {} addr, int offset) {{", addr_type)?;
        writeln!(file, "    return *(int32_t*)(mem + addr + offset);")?;
        writeln!(file, "}}")?;
        writeln!(file, "static inline uint32_t rd_mem_u32(uint8_t* mem, {} addr, int offset) {{", addr_type)?;
        writeln!(file, "    return *(uint32_t*)(mem + addr + offset);")?;
        writeln!(file, "}}")?;
        if X::VALUE == 64 {
            writeln!(file, "static inline uint64_t rd_mem_u64(uint8_t* mem, {} addr, int offset) {{", addr_type)?;
            writeln!(file, "    return *(uint64_t*)(mem + addr + offset);")?;
            writeln!(file, "}}")?;
        }
        writeln!(file)?;
        writeln!(file, "static inline void wr_mem_u8(uint8_t* mem, {} addr, uint8_t val) {{", addr_type)?;
        writeln!(file, "    *(uint8_t*)(mem + addr) = val;")?;
        writeln!(file, "}}")?;
        writeln!(file, "static inline void wr_mem_u16(uint8_t* mem, {} addr, uint16_t val) {{", addr_type)?;
        writeln!(file, "    *(uint16_t*)(mem + addr) = val;")?;
        writeln!(file, "}}")?;
        writeln!(file, "static inline void wr_mem_u32(uint8_t* mem, {} addr, uint32_t val) {{", addr_type)?;
        writeln!(file, "    *(uint32_t*)(mem + addr) = val;")?;
        writeln!(file, "}}")?;
        if X::VALUE == 64 {
            writeln!(file, "static inline void wr_mem_u64(uint8_t* mem, {} addr, uint64_t val) {{", addr_type)?;
            writeln!(file, "    *(uint64_t*)(mem + addr) = val;")?;
            writeln!(file, "}}")?;
        }
        writeln!(file)?;
        writeln!(file, "// Division helpers (handle divide-by-zero)")?;
        if X::VALUE == 64 {
            writeln!(file, "static inline int64_t rv_div64(int64_t a, int64_t b) {{ return b == 0 ? -1 : a / b; }}")?;
            writeln!(file, "static inline uint64_t rv_divu64(uint64_t a, uint64_t b) {{ return b == 0 ? ~0ULL : a / b; }}")?;
            writeln!(file, "static inline int64_t rv_rem64(int64_t a, int64_t b) {{ return b == 0 ? a : a % b; }}")?;
            writeln!(file, "static inline uint64_t rv_remu64(uint64_t a, uint64_t b) {{ return b == 0 ? a : a % b; }}")?;
        } else {
            writeln!(file, "static inline int32_t rv_div(int32_t a, int32_t b) {{ return b == 0 ? -1 : a / b; }}")?;
            writeln!(file, "static inline uint32_t rv_divu(uint32_t a, uint32_t b) {{ return b == 0 ? ~0U : a / b; }}")?;
            writeln!(file, "static inline int32_t rv_rem(int32_t a, int32_t b) {{ return b == 0 ? a : a % b; }}")?;
            writeln!(file, "static inline uint32_t rv_remu(uint32_t a, uint32_t b) {{ return b == 0 ? a : a % b; }}")?;
        }
        writeln!(file)?;
        writeln!(file, "// RV64 32-bit division")?;
        writeln!(file, "static inline uint64_t rv_divw(int32_t a, int32_t b) {{ return b == 0 ? -1LL : (int64_t)(a / b); }}")?;
        writeln!(file, "static inline uint64_t rv_divuw(uint32_t a, uint32_t b) {{ return b == 0 ? ~0ULL : (uint64_t)(a / b); }}")?;
        writeln!(file, "static inline uint64_t rv_remw(int32_t a, int32_t b) {{ return b == 0 ? (int64_t)a : (int64_t)(a % b); }}")?;
        writeln!(file, "static inline uint64_t rv_remuw(uint32_t a, uint32_t b) {{ return b == 0 ? (uint64_t)a : (uint64_t)(a % b); }}")?;
        writeln!(file)?;
        writeln!(file, "// CSR access stubs")?;
        writeln!(file, "static inline {} rd_csr(RvState* s, uint16_t csr) {{ return 0; }}", reg_type)?;
        writeln!(file, "static inline void wr_csr(RvState* s, uint16_t csr, {} val) {{}}", reg_type)?;
        writeln!(file)?;
        writeln!(file, "// Block declarations")?;

        // Declare all block functions
        let mut block_pcs: Vec<u64> = self.ir_blocks.keys().copied().collect();
        block_pcs.sort_unstable();
        for pc in &block_pcs {
            let pc_str = if X::VALUE == 64 {
                format!("{:016x}", pc)
            } else {
                format!("{:08x}", pc)
            };
            writeln!(file, "__attribute__((preserve_none)) void B_0x{}(RvState* state, uint8_t* memory, {} instret, {}* regs);", pc_str, reg_type, reg_type)?;
        }
        writeln!(file)?;
        writeln!(file, "// Dispatch")?;
        writeln!(file, "extern BlockFn dispatch_table[];")?;
        writeln!(file, "static inline size_t dispatch_index({} pc) {{ return (pc >> 1) & 0xFFFF; }}", addr_type)?;

        Ok(())
    }

    /// Emit partition files containing block implementations.
    fn emit_partitions(&self, output_dir: &Path, base_name: &str, emitter: &mut CEmitter<X>) -> Result<()> {
        // Get sorted blocks
        let mut block_pcs: Vec<u64> = self.ir_blocks.keys().copied().collect();
        block_pcs.sort_unstable();

        // Single partition for now
        let path = output_dir.join(format!("{}_part0.c", base_name));
        let mut file = std::fs::File::create(&path)?;

        writeln!(file, "#include \"{}.h\"", base_name)?;
        writeln!(file)?;

        for pc in &block_pcs {
            if let Some(block) = self.ir_blocks.get(pc) {
                emitter.reset();
                emitter.render_block(block);
                write!(file, "{}", emitter.output())?;
            }
        }

        Ok(())
    }

    /// Emit dispatch table.
    fn emit_dispatch(&self, output_dir: &Path, base_name: &str) -> Result<()> {
        let path = output_dir.join(format!("{}_dispatch.c", base_name));
        let mut file = std::fs::File::create(&path)?;

        let reg_type = if X::VALUE == 64 { "uint64_t" } else { "uint32_t" };

        writeln!(file, "#include \"{}.h\"", base_name)?;
        writeln!(file)?;

        // Error handler for invalid addresses
        writeln!(file, "__attribute__((preserve_none)) void B_invalid(RvState* state, uint8_t* memory, {} instret, {}* regs) {{", reg_type, reg_type)?;
        writeln!(file, "    state->has_exited = true;")?;
        writeln!(file, "    state->exit_code = 1;")?;
        writeln!(file, "}}")?;
        writeln!(file)?;

        // Build dispatch table (sparse table indexed by PC >> 1)
        writeln!(file, "BlockFn dispatch_table[65536] = {{")?;

        // Get all valid PCs
        let mut block_pcs: Vec<u64> = self.ir_blocks.keys().copied().collect();
        block_pcs.sort_unstable();

        // Fill table
        for i in 0..65536u32 {
            // Find a block that maps to this index
            let mut found = false;
            for pc in &block_pcs {
                if ((*pc >> 1) & 0xFFFF) as u32 == i {
                    let pc_str = if X::VALUE == 64 {
                        format!("{:016x}", pc)
                    } else {
                        format!("{:08x}", pc)
                    };
                    writeln!(file, "    [{}] = B_0x{},", i, pc_str)?;
                    found = true;
                    break;
                }
            }
            if !found && i == 0 {
                // Default entry
                writeln!(file, "    [0] = B_invalid,")?;
            }
        }

        writeln!(file, "}};")?;
        writeln!(file)?;

        // Entry point function
        let entry = X::to_u64(self.image.entry_point);
        let entry_str = if X::VALUE == 64 {
            format!("{:016x}", entry)
        } else {
            format!("{:08x}", entry)
        };
        writeln!(file, "void rv_execute(RvState* state, uint8_t* memory) {{")?;
        writeln!(file, "    B_0x{}(state, memory, 0, state->regs);", entry_str)?;
        writeln!(file, "}}")?;

        Ok(())
    }

    /// Emit Makefile.
    fn emit_makefile(&self, output_dir: &Path, base_name: &str) -> Result<()> {
        let path = output_dir.join("Makefile");
        let mut file = std::fs::File::create(&path)?;

        writeln!(file, "# Generated by RVR")?;
        writeln!(file, "CC = clang")?;
        writeln!(file, "CFLAGS = -O3 -fPIC -Wall -Wno-unused-function")?;
        writeln!(file)?;
        writeln!(file, "SRCS = {}_part0.c {}_dispatch.c", base_name, base_name)?;
        writeln!(file, "OBJS = $(SRCS:.c=.o)")?;
        writeln!(file)?;
        writeln!(file, "shared: lib{}.so", base_name)?;
        writeln!(file)?;
        writeln!(file, "lib{}.so: $(OBJS)", base_name)?;
        writeln!(file, "\t$(CC) -shared -o $@ $(OBJS)")?;
        writeln!(file)?;
        writeln!(file, "%.o: %.c {}.h", base_name)?;
        writeln!(file, "\t$(CC) $(CFLAGS) -c $< -o $@")?;
        writeln!(file)?;
        writeln!(file, "clean:")?;
        writeln!(file, "\trm -f $(OBJS) lib{}.so", base_name)?;
        writeln!(file)?;
        writeln!(file, ".PHONY: shared clean")?;

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
