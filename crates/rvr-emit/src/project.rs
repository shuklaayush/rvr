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
use tracing::{debug, info, trace};

use crate::config::{EmitConfig, SyscallMode};
use crate::dispatch::{DispatchConfig, gen_dispatch_file};
use crate::emitter::CEmitter;
use crate::header::{HeaderConfig, gen_blocks_header, gen_header};
use crate::htif::{HtifConfig, gen_htif_header, gen_htif_source};
use crate::inputs::EmitInputs;
use crate::memory::{MemoryConfig, MemorySegment, gen_memory_file_with_embed, gen_segment_bins};
use crate::syscalls::{SyscallsConfig, gen_syscalls_source};
use crate::tracer::gen_tracer_header;

/// Default instructions per partition.
pub const DEFAULT_PARTITION_SIZE: usize = 8192;

/// C code generation project.
pub struct CProject<X: Xlen> {
    /// Output directory.
    pub output_dir: PathBuf,
    /// Base name for generated files.
    pub base_name: String,
    /// Emit configuration (includes compiler choice).
    pub config: EmitConfig<X>,
    /// Derived inputs for emission (entry point, pc_end, valid addresses, initial_brk).
    pub inputs: EmitInputs,
    /// Taken-inline mapping: branch_pc -> (inline_start, inline_end).
    pub taken_inlines: HashMap<u64, (u64, u64)>,
    /// Memory segments.
    pub segments: Vec<MemorySegment>,
    /// Instructions per partition.
    pub partition_size: usize,
    /// Enable LTO.
    pub enable_lto: bool,
    /// Number of parallel compilation jobs.
    pub jobs: usize,
}

impl<X: Xlen> CProject<X> {
    /// Create a new CProject.
    pub fn new(
        output_dir: impl AsRef<Path>,
        base_name: impl Into<String>,
        config: EmitConfig<X>,
    ) -> Self {
        // Default to nproc-2 to leave headroom for system
        let jobs = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).max(1))
            .unwrap_or(4);
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            base_name: base_name.into(),
            config,
            inputs: EmitInputs::default(),
            taken_inlines: HashMap::new(),
            segments: Vec::new(),
            partition_size: DEFAULT_PARTITION_SIZE,
            enable_lto: true,
            jobs,
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
    pub fn with_compiler(mut self, compiler: crate::Compiler) -> Self {
        self.config.compiler = compiler;
        self
    }

    /// Set number of parallel compilation jobs.
    pub fn with_jobs(mut self, jobs: usize) -> Self {
        self.jobs = jobs;
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
        self.output_dir
            .join(format!("{}_syscalls.c", self.base_name))
    }

    /// Path to tracer header file.
    pub fn tracer_header_path(&self) -> PathBuf {
        self.output_dir.join("rv_tracer.h")
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
        let header_path = self.header_path();
        trace!(path = %header_path.display(), "writing header");
        fs::write(&header_path, header)?;

        let blocks_header = gen_blocks_header::<X>(&header_cfg);
        let blocks_path = self.blocks_header_path();
        trace!(path = %blocks_path.display(), "writing blocks header");
        fs::write(&blocks_path, blocks_header)?;

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

            emitter.render_block_header_with_count(start_pc, end_pc, num_instrs);
            emitter.render_instret_check(start_pc);

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
                        // Emit trace_pc for the branch instruction before rendering its statements
                        emitter.emit_trace_pc_for(X::to_u64(last_instr.pc), last_instr.op);

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
                            // Note: instret update is handled inside render_instruction_indented for is_last=true
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

            // Note: instret update is already done inside render_instruction for is_last=true

            emitter.render_block_footer();
            content.push_str(emitter.output());
        }

        let path = self.partition_path(partition_idx);
        trace!(path = %path.display(), blocks = blocks.len(), "writing partition");
        fs::write(path, content)
    }

    /// Write all partition files.
    pub fn write_partitions(&self, blocks: &[BlockIR<X>]) -> std::io::Result<usize> {
        // Build block lookup map for taken-inline support
        let block_map: HashMap<u64, &BlockIR<X>> =
            blocks.iter().map(|b| (X::to_u64(b.start_pc), b)).collect();

        let partitions = self.partition_blocks(blocks);
        let num_partitions = partitions.len();

        debug!(
            total_blocks = blocks.len(),
            partitions = num_partitions,
            partition_size = self.partition_size,
            "partitioning blocks"
        );

        for (idx, partition_blocks) in partitions {
            self.write_partition(idx, &partition_blocks, &block_map)?;
        }

        Ok(num_partitions)
    }

    /// Write dispatch file.
    pub fn write_dispatch(&self) -> std::io::Result<()> {
        let dispatch_cfg = DispatchConfig::new(&self.config, &self.base_name, self.inputs.clone());

        let dispatch = gen_dispatch_file::<X>(&dispatch_cfg);
        let path = self.dispatch_path();
        trace!(path = %path.display(), "writing dispatch");
        fs::write(path, dispatch)
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
            let path = self.output_dir.join(&name);
            trace!(path = %path.display(), size = data.len(), "writing segment binary");
            fs::write(path, data)?;
        }

        let memory = gen_memory_file_with_embed(&mem_cfg);
        let path = self.memory_path();
        trace!(path = %path.display(), segments = self.segments.len(), "writing memory");
        fs::write(path, memory)
    }

    /// Write HTIF files.
    pub fn write_htif(&self) -> std::io::Result<()> {
        let htif_cfg = HtifConfig::new(&self.base_name, self.config.htif_enabled)
            .with_verbose(self.config.htif_verbose);

        let htif_header = gen_htif_header::<X>(&htif_cfg);
        let header_path = self.htif_header_path();
        trace!(path = %header_path.display(), "writing htif header");
        fs::write(header_path, htif_header)?;

        let htif_source = gen_htif_source::<X>(&htif_cfg);
        let src_path = self.htif_source_path();
        trace!(path = %src_path.display(), "writing htif source");
        fs::write(src_path, htif_source)?;

        Ok(())
    }

    /// Write syscall runtime source.
    pub fn write_syscalls(&self) -> std::io::Result<()> {
        let cfg = SyscallsConfig::new(&self.base_name, self.config.fixed_addresses.is_some());
        let src = gen_syscalls_source::<X>(&cfg);
        let path = self.syscalls_path();
        trace!(path = %path.display(), "writing syscalls");
        fs::write(path, src)
    }

    /// Write tracer header if tracing is enabled.
    pub fn write_tracer_header(&self) -> std::io::Result<()> {
        if self.config.tracer_config.is_none() {
            return Ok(());
        }
        let tracer_header = gen_tracer_header::<X>(&self.config.tracer_config)?;
        let path = self.tracer_header_path();
        trace!(path = %path.display(), "writing tracer header");
        fs::write(path, tracer_header)
    }

    /// Write Makefile.
    ///
    /// Compiler flags are determined in Rust based on the compiler field:
    /// - clang: uses `-std=c23`, `-flto=thin`, `-fuse-ld=lld`, `-fzero-call-used-regs=skip`
    /// - gcc: uses `-std=c2x`, `-flto`, omits clang-specific flags
    pub fn write_makefile(&self, num_partitions: usize) -> std::io::Result<()> {
        let mut content = String::new();

        let compiler = &self.config.compiler;
        let is_clang = compiler.is_clang();

        writeln!(content, "# Generated by RVR").unwrap();
        writeln!(content).unwrap();

        // Limit parallel jobs (computed in Rust as nproc-2 by default)
        writeln!(content, "MAKEFLAGS += -j{} -l{}", self.jobs, self.jobs).unwrap();
        writeln!(content).unwrap();

        writeln!(content, "CC = {}", compiler).unwrap();
        writeln!(content).unwrap();

        // Build CFLAGS based on compiler type (determined in Rust)
        let mut cflags = vec![
            "-O3",
            "-march=native",
            "-pipe",
            "-fomit-frame-pointer",
            "-funroll-loops",
            "-fno-stack-protector",
            "-w",
            "-DNDEBUG",
        ];

        let mut ldflags: Vec<String> = Vec::new();

        if is_clang {
            cflags.push("-std=c23");
            cflags.push("-fzero-call-used-regs=skip");
            if self.enable_lto {
                cflags.push("-flto=thin");
                cflags.push("-fno-plt");
                cflags.push("-fno-semantic-interposition");
                ldflags.push("-flto=thin".to_string());
                if let Some(linker) = compiler.linker() {
                    ldflags.push(format!("-fuse-ld={}", linker));
                }
            }
        } else {
            // GCC
            cflags.push("-std=c2x");
            if self.enable_lto {
                cflags.push("-flto");
                ldflags.push("-flto".to_string());
            }
        }

        writeln!(content, "CFLAGS = {}", cflags.join(" ")).unwrap();
        if !ldflags.is_empty() {
            writeln!(content, "LDFLAGS = {}", ldflags.join(" ")).unwrap();
        } else {
            writeln!(content, "LDFLAGS =").unwrap();
        }
        writeln!(content, "SHARED_FLAGS = -fPIC").unwrap();
        writeln!(content).unwrap();

        // Source files
        let mut srcs: Vec<String> = (0..num_partitions)
            .map(|i| format!("{}_part{}.c", self.base_name, i))
            .collect();
        srcs.push(format!("{}_dispatch.c", self.base_name));
        if self.config.syscall_mode == SyscallMode::Linux {
            srcs.push(format!("{}_syscalls.c", self.base_name));
        }
        if !self.segments.is_empty() {
            srcs.push(format!("{}_memory.c", self.base_name));
        }
        if self.config.htif_enabled {
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

        let path = self.makefile_path();
        trace!(path = %path.display(), "writing Makefile");
        fs::write(path, content)
    }

    /// Write all files.
    ///
    /// Returns the number of partitions created.
    pub fn write_all(&self, blocks: &[BlockIR<X>]) -> std::io::Result<usize> {
        // Ensure output directory exists
        fs::create_dir_all(&self.output_dir)?;

        debug!(
            output_dir = %self.output_dir.display(),
            base_name = %self.base_name,
            blocks = blocks.len(),
            "generating C project"
        );

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
        if self.config.htif_enabled {
            self.write_htif()?;
        }

        // Write syscall runtime (only for Linux mode)
        if self.config.syscall_mode == SyscallMode::Linux {
            self.write_syscalls()?;
        }

        // Write tracer header if tracing enabled
        self.write_tracer_header()?;

        // Write Makefile
        self.write_makefile(num_partitions)?;

        info!(
            output_dir = %self.output_dir.display(),
            partitions = num_partitions,
            "C project generated"
        );

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
