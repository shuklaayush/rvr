//! CProject - C code generation orchestration.
//!
//! Coordinates emission of all C files:
//! - Header files (main header + blocks header)
//! - Partition files (blocks split by instruction count)
//! - Dispatch table
//! - Memory initialization
//! - Makefile

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};

use rvr_ir::{BlockIR, Xlen};

use crate::config::EmitConfig;
use crate::dispatch::{gen_dispatch_file, DispatchConfig};
use crate::emitter::CEmitter;
use crate::header::{gen_blocks_header, gen_header, HeaderConfig};
use crate::htif::{gen_htif_header, gen_htif_source, HtifConfig};
use crate::inputs::EmitInputs;
use crate::memory::{gen_memory_file_with_embed, gen_segment_bins, MemoryConfig, MemorySegment};
use crate::syscalls::{gen_syscalls_source, SyscallsConfig};
use crate::tracer::gen_tracer_header;
use crate::tracer::{PassedVarKind, TracerConfig};

/// Default instructions per partition.
pub const DEFAULT_PARTITION_SIZE: usize = 8192;

/// C code generation project.
pub struct CProject<X: Xlen> {
    /// Output directory.
    pub output_dir: PathBuf,
    /// Base name for generated files.
    pub base_name: String,
    /// Emit configuration.
    pub config: EmitConfig<X>,
    /// Derived inputs for emission (entry point, pc_end, valid addresses, initial_brk).
    pub inputs: EmitInputs,
    /// Taken-inline mapping: branch_pc -> (inline_start, inline_end).
    pub taken_inlines: HashMap<u64, (u64, u64)>,
    /// Memory segments.
    pub segments: Vec<MemorySegment>,
    /// Instructions per partition.
    pub partition_size: usize,
    /// Compiler command.
    pub compiler: String,
    /// Enable LTO.
    pub enable_lto: bool,
}

impl<X: Xlen> CProject<X> {
    /// Create a new CProject.
    pub fn new(
        output_dir: impl AsRef<Path>,
        base_name: impl Into<String>,
        config: EmitConfig<X>,
    ) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            base_name: base_name.into(),
            config,
            inputs: EmitInputs::default(),
            taken_inlines: HashMap::new(),
            segments: Vec::new(),
            partition_size: DEFAULT_PARTITION_SIZE,
            compiler: "clang".to_string(),
            enable_lto: true,
        }
    }

    /// Set derived emission inputs.
    pub fn with_inputs(mut self, inputs: EmitInputs) -> Self {
        self.inputs = inputs;
        self
    }

    /// Set taken-inline mapping for branch inlining.
    pub fn with_taken_inlines(mut self, mapping: HashMap<u64, (u64, u64)>) -> Self {
        self.taken_inlines = mapping;
        self
    }

    /// Set memory segments.
    pub fn with_segments(mut self, segments: Vec<MemorySegment>) -> Self {
        self.segments = segments;
        self
    }

    /// Set partition size.
    pub fn with_partition_size(mut self, size: usize) -> Self {
        self.partition_size = size;
        self
    }

    /// Set compiler.
    pub fn with_compiler(mut self, compiler: impl Into<String>) -> Self {
        self.compiler = compiler.into();
        self
    }

    // ============= Path helpers =============

    /// Path to main header file.
    pub fn header_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}.h", self.base_name))
    }

    /// Path to blocks header file.
    pub fn blocks_header_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}_blocks.h", self.base_name))
    }

    /// Path to partition file.
    pub fn partition_path(&self, idx: usize) -> PathBuf {
        self.output_dir
            .join(format!("{}_part{}.c", self.base_name, idx))
    }

    /// Path to dispatch file.
    pub fn dispatch_path(&self) -> PathBuf {
        self.output_dir
            .join(format!("{}_dispatch.c", self.base_name))
    }

    /// Path to memory file.
    pub fn memory_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}_memory.c", self.base_name))
    }

    /// Path to HTIF header file.
    pub fn htif_header_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}_htif.h", self.base_name))
    }

    /// Path to HTIF source file.
    pub fn htif_source_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}_htif.c", self.base_name))
    }

    /// Path to syscalls source file.
    pub fn syscalls_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}_syscalls.c", self.base_name))
    }

    /// Path to tracer header file.
    pub fn tracer_header_path(&self) -> PathBuf {
        self.output_dir.join("rv_tracer.h")
    }

    /// Path to metadata file.
    pub fn meta_path(&self) -> PathBuf {
        self.output_dir.join("rvr_meta.json")
    }

    /// Path to Makefile.
    pub fn makefile_path(&self) -> PathBuf {
        self.output_dir.join("Makefile")
    }

    /// Path to shared library.
    pub fn shared_lib_path(&self) -> PathBuf {
        self.output_dir.join(format!("lib{}.so", self.base_name))
    }

    // ============= File generation =============

    /// Write main header file.
    pub fn write_header(&self, block_addresses: &[u64]) -> std::io::Result<()> {
        let header_cfg = HeaderConfig::new(
            &self.base_name,
            &self.config,
            &self.inputs,
            block_addresses.to_vec(),
        );

        let header = gen_header::<X>(&header_cfg);
        fs::write(self.header_path(), header)?;

        let blocks_header = gen_blocks_header::<X>(&header_cfg);
        fs::write(self.blocks_header_path(), blocks_header)?;

        Ok(())
    }

    /// Partition blocks by instruction count.
    ///
    /// Returns list of (partition_idx, blocks) tuples.
    pub fn partition_blocks<'a>(
        &self,
        blocks: &'a [BlockIR<X>],
    ) -> Vec<(usize, Vec<&'a BlockIR<X>>)> {
        let mut partitions = Vec::new();
        let mut current_partition = Vec::new();
        let mut current_count = 0;
        let mut partition_idx = 0;

        for block in blocks {
            let instr_count = block.instructions.len();

            // Start new partition if this would exceed limit
            if !current_partition.is_empty() && current_count + instr_count > self.partition_size {
                partitions.push((partition_idx, current_partition));
                current_partition = Vec::new();
                current_count = 0;
                partition_idx += 1;
            }

            current_partition.push(block);
            current_count += instr_count;
        }

        // Add final partition
        if !current_partition.is_empty() {
            partitions.push((partition_idx, current_partition));
        }

        partitions
    }

    /// Write partition file.
    ///
    /// The `block_map` is used for taken-inline support - when a branch has an
    /// inline entry, we look up the inlined block by its start address.
    pub fn write_partition(
        &self,
        partition_idx: usize,
        blocks: &[&BlockIR<X>],
        block_map: &HashMap<u64, &BlockIR<X>>,
    ) -> std::io::Result<()> {
        use rvr_ir::Terminator;

        let mut emitter = CEmitter::new(self.config.clone(), self.inputs.clone());
        let mut content = String::new();

        content.push_str(&format!("#include \"{}_blocks.h\"\n\n", self.base_name));

        for block in blocks {
            emitter.reset();

            let start_pc = X::to_u64(block.start_pc);
            let end_pc = X::to_u64(block.end_pc);
            let num_instrs = block.instructions.len();

            emitter.render_block_header(start_pc, end_pc);

            if num_instrs == 0 {
                emitter.render_block_footer();
                content.push_str(emitter.output());
                continue;
            }

            // Get the last instruction to check for taken-inline
            let last_instr = block.instructions.last().unwrap();
            let last_pc = X::to_u64(last_instr.pc);

            // Check if this block's last instruction is a branch with taken-inline
            let taken_inline = if let Terminator::Branch { cond, hint, .. } = &last_instr.terminator
            {
                if let Some(&(inline_start, _inline_end)) = self.taken_inlines.get(&last_pc) {
                    // Found a taken-inline entry for this branch
                    Some((cond.clone(), *hint, inline_start))
                } else {
                    None
                }
            } else {
                None
            };

            // Render all instructions except the last one normally
            for (i, instr) in block.instructions.iter().enumerate() {
                let is_last = i == num_instrs - 1;
                if is_last {
                    // Handle last instruction specially if it has taken-inline
                    if let Some((cond, hint, inline_start)) = &taken_inline {
                        // Render the last instruction's statements (but not terminator)
                        for stmt in &last_instr.statements {
                            emitter.render_stmt(stmt, 1);
                        }

                        // Render branch condition open
                        let cond_str = emitter.render_expr(cond);
                        emitter.render_branch_open(&cond_str, *hint);

                        // Look up and render the inlined block
                        if let Some(inline_block) = block_map.get(inline_start) {
                            let inline_num_instrs = inline_block.instructions.len();
                            let inline_end_pc = X::to_u64(inline_block.end_pc);

                            // Render inlined instructions with extra indent
                            for (j, inline_instr) in inline_block.instructions.iter().enumerate() {
                                let is_inline_last = j == inline_num_instrs - 1;
                                emitter.render_instruction_indented(
                                    inline_instr,
                                    is_inline_last,
                                    inline_end_pc,
                                    2, // Extra indentation for if-block
                                );
                            }

                            // Update instret for inlined instructions
                            if inline_num_instrs > 0 {
                                emitter.render_instret_update_indented(inline_num_instrs as u64, 2);
                            }
                        }

                        // Close branch if-block
                        emitter.render_branch_close();

                        // Fall-through for not-taken path
                        emitter.render_jump_static(end_pc);
                    } else {
                        // No taken-inline, render normally
                        emitter.render_instruction(instr, is_last, end_pc);
                    }
                } else {
                    emitter.render_instruction(instr, false, end_pc);
                }
            }

            // Update instret for main block
            if num_instrs > 0 {
                emitter.render_instret_update(num_instrs as u64);
            }

            emitter.render_block_footer();
            content.push_str(emitter.output());
        }

        fs::write(self.partition_path(partition_idx), content)
    }

    /// Write all partition files.
    pub fn write_partitions(&self, blocks: &[BlockIR<X>]) -> std::io::Result<usize> {
        // Build block lookup map for taken-inline support
        let block_map: HashMap<u64, &BlockIR<X>> =
            blocks.iter().map(|b| (X::to_u64(b.start_pc), b)).collect();

        let partitions = self.partition_blocks(blocks);
        let num_partitions = partitions.len();

        for (idx, partition_blocks) in partitions {
            self.write_partition(idx, &partition_blocks, &block_map)?;
        }

        Ok(num_partitions)
    }

    /// Write dispatch file.
    pub fn write_dispatch(&self) -> std::io::Result<()> {
        let dispatch_cfg = DispatchConfig::new(&self.config, &self.base_name, self.inputs.clone());

        let dispatch = gen_dispatch_file::<X>(&dispatch_cfg);
        fs::write(self.dispatch_path(), dispatch)
    }

    /// Write memory file.
    pub fn write_memory(&self) -> std::io::Result<()> {
        let mem_cfg = MemoryConfig::new(
            &self.base_name,
            self.segments.clone(),
            self.config.memory_bits,
            self.inputs.initial_brk,
        );

        // C23 #embed: write segment binaries, then emit memory.c with #embed directives.
        for (name, data) in gen_segment_bins(&mem_cfg) {
            let path = self.output_dir.join(name);
            fs::write(path, data)?;
        }

        let memory = gen_memory_file_with_embed(&mem_cfg);
        fs::write(self.memory_path(), memory)
    }

    /// Write HTIF files.
    pub fn write_htif(&self) -> std::io::Result<()> {
        let htif_cfg = HtifConfig::new(&self.base_name, self.config.tohost_enabled);

        let htif_header = gen_htif_header::<X>(&htif_cfg);
        fs::write(self.htif_header_path(), htif_header)?;

        let htif_source = gen_htif_source::<X>(&htif_cfg);
        fs::write(self.htif_source_path(), htif_source)?;

        Ok(())
    }

    /// Write syscall runtime source.
    pub fn write_syscalls(&self) -> std::io::Result<()> {
        let cfg = SyscallsConfig::new(&self.base_name);
        let src = gen_syscalls_source::<X>(&cfg);
        fs::write(self.syscalls_path(), src)
    }

    /// Write tracer header if tracing is enabled.
    pub fn write_tracer_header(&self) -> std::io::Result<()> {
        if self.config.tracer_config.is_none() {
            return Ok(());
        }
        let tracer_header = gen_tracer_header::<X>(&self.config.tracer_config)?;
        fs::write(self.tracer_header_path(), tracer_header)
    }

    /// Write emission metadata for runtime tools.
    pub fn write_meta(&self) -> std::io::Result<()> {
        let cfg: &TracerConfig = &self.config.tracer_config;
        let mut passed = String::new();
        for (idx, var) in cfg.passed_vars.iter().enumerate() {
            let kind = match var.kind {
                PassedVarKind::Ptr => "ptr",
                PassedVarKind::Index => "index",
                PassedVarKind::Value => "value",
            };
            if idx > 0 {
                passed.push_str(", ");
            }
            passed.push_str(&format!(
                r#"{{"name":"{}","kind":"{}"}}"#,
                var.name, kind
            ));
        }

        let content = format!(
            r#"{{"xlen":{},"tracer_kind":"{}","tracer_passed":[{}]}}"#,
            X::VALUE,
            cfg.meta_kind(),
            passed
        );

        fs::write(self.meta_path(), content)
    }

    /// Write Makefile.
    ///
    /// Generated Makefile includes compiler detection for clang vs gcc compatibility:
    /// - clang: uses `-std=c23`, `-flto=thin`, `-fuse-ld=lld`, `-fzero-call-used-regs=skip`
    /// - gcc: uses `-std=c2x`, `-flto`, omits clang-specific flags
    pub fn write_makefile(&self, num_partitions: usize) -> std::io::Result<()> {
        let mut content = String::new();

        writeln!(content, "# Generated by RVR").unwrap();
        writeln!(content, "CC ?= {}", self.compiler).unwrap();
        writeln!(content).unwrap();

        // Compiler detection - check if using clang
        writeln!(
            content,
            "# Compiler detection for clang vs gcc compatibility"
        )
        .unwrap();
        writeln!(
            content,
            "IS_CLANG := $(shell $(CC) --version 2>/dev/null | grep -c clang)"
        )
        .unwrap();
        writeln!(content).unwrap();

        // Base flags: optimization and performance settings
        // These work on both clang and gcc
        writeln!(content, "# Base flags (clang and gcc compatible)").unwrap();
        let base_cflags = vec![
            "-O3",
            "-march=native",
            "-pipe",
            "-fomit-frame-pointer",
            "-funroll-loops",
            "-fno-stack-protector",
            "-w", // Suppress warnings (generated code is known-correct)
            "-DNDEBUG",
        ];
        writeln!(content, "BASE_CFLAGS = {}", base_cflags.join(" ")).unwrap();
        writeln!(content).unwrap();

        // Clang-specific flags
        writeln!(content, "# Clang-specific flags").unwrap();
        let mut clang_flags = vec!["-std=c23", "-fzero-call-used-regs=skip"];
        if self.enable_lto {
            clang_flags.push("-flto=thin");
            clang_flags.push("-fno-plt");
            clang_flags.push("-fno-semantic-interposition");
        }
        writeln!(content, "CLANG_CFLAGS = {}", clang_flags.join(" ")).unwrap();
        if self.enable_lto {
            writeln!(content, "CLANG_LDFLAGS = -flto=thin -fuse-ld=lld").unwrap();
        } else {
            writeln!(content, "CLANG_LDFLAGS =").unwrap();
        }
        writeln!(content).unwrap();

        // GCC-specific flags
        writeln!(content, "# GCC-specific flags").unwrap();
        let mut gcc_flags = vec!["-std=c2x"]; // c2x is gcc's C23 draft
        if self.enable_lto {
            gcc_flags.push("-flto");
        }
        writeln!(content, "GCC_CFLAGS = {}", gcc_flags.join(" ")).unwrap();
        if self.enable_lto {
            writeln!(content, "GCC_LDFLAGS = -flto").unwrap();
        } else {
            writeln!(content, "GCC_LDFLAGS =").unwrap();
        }
        writeln!(content).unwrap();

        // Conditional flag selection
        writeln!(content, "# Select flags based on compiler").unwrap();
        writeln!(content, "ifeq ($(IS_CLANG),1)").unwrap();
        writeln!(
            content,
            "  CFLAGS = $(BASE_CFLAGS) $(CLANG_CFLAGS)"
        )
        .unwrap();
        writeln!(content, "  LDFLAGS = $(CLANG_LDFLAGS)").unwrap();
        writeln!(content, "else").unwrap();
        writeln!(content, "  CFLAGS = $(BASE_CFLAGS) $(GCC_CFLAGS)").unwrap();
        writeln!(content, "  LDFLAGS = $(GCC_LDFLAGS)").unwrap();
        writeln!(content, "endif").unwrap();
        writeln!(content).unwrap();

        writeln!(content, "SHARED_FLAGS = -fPIC").unwrap();
        writeln!(content).unwrap();

        // Source files
        let mut srcs: Vec<String> = (0..num_partitions)
            .map(|i| format!("{}_part{}.c", self.base_name, i))
            .collect();
        srcs.push(format!("{}_dispatch.c", self.base_name));
        srcs.push(format!("{}_syscalls.c", self.base_name));
        if !self.segments.is_empty() {
            srcs.push(format!("{}_memory.c", self.base_name));
        }
        if self.config.tohost_enabled {
            srcs.push(format!("{}_htif.c", self.base_name));
        }

        writeln!(content, "SRCS = {}", srcs.join(" ")).unwrap();
        writeln!(content, "OBJS = $(SRCS:.c=.o)").unwrap();
        writeln!(content).unwrap();

        // Targets
        writeln!(content, "shared: lib{}.so", self.base_name).unwrap();
        writeln!(content).unwrap();

        writeln!(content, "lib{}.so: $(OBJS)", self.base_name).unwrap();
        // Always use LDFLAGS - it may be empty if LTO disabled
        writeln!(
            content,
            "\t$(CC) $(CFLAGS) $(LDFLAGS) -shared -o $@ $(OBJS)"
        )
        .unwrap();
        writeln!(content).unwrap();

        writeln!(
            content,
            "%.o: %.c {}.h {}_blocks.h",
            self.base_name, self.base_name
        )
        .unwrap();
        writeln!(content, "\t$(CC) $(CFLAGS) $(SHARED_FLAGS) -c $< -o $@").unwrap();
        writeln!(content).unwrap();

        writeln!(content, "clean:").unwrap();
        writeln!(content, "\trm -f $(OBJS) lib{}.so", self.base_name).unwrap();
        writeln!(content).unwrap();

        writeln!(content, ".PHONY: shared clean").unwrap();

        fs::write(self.makefile_path(), content)
    }

    /// Write all files.
    ///
    /// Returns the number of partitions created.
    pub fn write_all(&self, blocks: &[BlockIR<X>]) -> std::io::Result<usize> {
        // Ensure output directory exists
        fs::create_dir_all(&self.output_dir)?;

        // Collect block addresses
        let block_addresses: Vec<u64> = blocks.iter().map(|b| X::to_u64(b.start_pc)).collect();

        // Write header
        self.write_header(&block_addresses)?;

        // Write partitions
        let num_partitions = self.write_partitions(blocks)?;

        // Write dispatch
        self.write_dispatch()?;

        // Write memory if segments exist
        if !self.segments.is_empty() {
            self.write_memory()?;
        }

        // Write HTIF if tohost enabled
        if self.config.tohost_enabled {
            self.write_htif()?;
        }

        // Write syscall runtime
        self.write_syscalls()?;

        // Write tracer header if tracing enabled
        self.write_tracer_header()?;

        // Write metadata for runtime tools
        self.write_meta()?;

        // Write Makefile
        self.write_makefile(num_partitions)?;

        Ok(num_partitions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_project_paths() {
        let config = EmitConfig::<Rv64>::default();
        let project = CProject::new("/tmp/test", "rv64", config);

        assert_eq!(project.header_path().to_str().unwrap(), "/tmp/test/rv64.h");
        assert_eq!(
            project.blocks_header_path().to_str().unwrap(),
            "/tmp/test/rv64_blocks.h"
        );
        assert_eq!(
            project.partition_path(0).to_str().unwrap(),
            "/tmp/test/rv64_part0.c"
        );
        assert_eq!(
            project.dispatch_path().to_str().unwrap(),
            "/tmp/test/rv64_dispatch.c"
        );
        assert_eq!(
            project.syscalls_path().to_str().unwrap(),
            "/tmp/test/rv64_syscalls.c"
        );
        assert_eq!(
            project.makefile_path().to_str().unwrap(),
            "/tmp/test/Makefile"
        );
    }

    #[test]
    fn test_partition_blocks() {
        let config = EmitConfig::<Rv64>::default();
        let project = CProject::new("/tmp/test", "rv64", config).with_partition_size(10);

        // Create dummy blocks with different instruction counts
        let blocks: Vec<BlockIR<Rv64>> = vec![
            create_dummy_block(0x1000, 5), // 5 instructions
            create_dummy_block(0x2000, 3), // 3 instructions -> partition 0 (8 total)
            create_dummy_block(0x3000, 4), // 4 instructions -> partition 1 (4 total)
            create_dummy_block(0x4000, 6), // 6 instructions -> partition 1 (10 total)
            create_dummy_block(0x5000, 2), // 2 instructions -> partition 2
        ];

        let partitions = project.partition_blocks(&blocks);

        assert_eq!(partitions.len(), 3);
        assert_eq!(partitions[0].0, 0);
        assert_eq!(partitions[0].1.len(), 2); // blocks 0, 1
        assert_eq!(partitions[1].0, 1);
        assert_eq!(partitions[1].1.len(), 2); // blocks 2, 3
        assert_eq!(partitions[2].0, 2);
        assert_eq!(partitions[2].1.len(), 1); // block 4
    }

    fn create_dummy_block(start_pc: u64, num_instrs: usize) -> BlockIR<Rv64> {
        use rvr_ir::{InstrIR, Terminator};

        let mut block = BlockIR::new(start_pc);
        for i in 0..num_instrs {
            let pc = start_pc + (i as u64 * 4);
            let ir = InstrIR::new(pc, 4, 0, Vec::new(), Terminator::default());
            block.push(ir);
        }
        block
    }
}
