//! CProject - C code generation orchestration.
//!
//! Coordinates emission of all C files:
//! - Header files (main header + blocks header)
//! - Partition files (blocks split by instruction count)
//! - Dispatch table
//! - Memory initialization
//! - Makefile

use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};

use rvr_ir::{BlockIR, Xlen};

use crate::config::EmitConfig;
use crate::dispatch::{DispatchConfig, gen_dispatch_file};
use crate::emitter::CEmitter;
use crate::header::{HeaderConfig, gen_header, gen_blocks_header};
use crate::memory::{MemoryConfig, MemorySegment, gen_memory_file};

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
    /// Valid block start addresses.
    pub valid_addresses: HashSet<u64>,
    /// Absorbed block mapping: absorbed_pc -> merged_block_start.
    pub absorbed_to_merged: HashMap<u64, u64>,
    /// Program entry point.
    pub entry_point: u64,
    /// End address (exclusive).
    pub pc_end: u64,
    /// Initial brk value.
    pub initial_brk: u64,
    /// Memory segments.
    pub segments: Vec<MemorySegment>,
    /// Instructions per partition.
    pub partition_size: usize,
    /// Compiler command.
    pub compiler: String,
    /// Enable LTO.
    pub enable_lto: bool,
    /// Enable tohost check.
    pub enable_tohost: bool,
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
            valid_addresses: HashSet::new(),
            absorbed_to_merged: HashMap::new(),
            entry_point: 0,
            pc_end: 0,
            initial_brk: 0,
            segments: Vec::new(),
            partition_size: DEFAULT_PARTITION_SIZE,
            compiler: "clang".to_string(),
            enable_lto: true,
            enable_tohost: false,
        }
    }

    /// Set entry point.
    pub fn with_entry_point(mut self, entry: u64) -> Self {
        self.entry_point = entry;
        self
    }

    /// Set PC end.
    pub fn with_pc_end(mut self, pc_end: u64) -> Self {
        self.pc_end = pc_end;
        self
    }

    /// Set initial brk.
    pub fn with_initial_brk(mut self, brk: u64) -> Self {
        self.initial_brk = brk;
        self
    }

    /// Set valid addresses.
    pub fn with_valid_addresses(mut self, addrs: HashSet<u64>) -> Self {
        self.valid_addresses = addrs;
        self
    }

    /// Set absorbed to merged mapping.
    pub fn with_absorbed_mapping(mut self, mapping: HashMap<u64, u64>) -> Self {
        self.absorbed_to_merged = mapping;
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

    /// Set tohost enabled.
    pub fn with_tohost(mut self, enabled: bool) -> Self {
        self.enable_tohost = enabled;
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
        self.output_dir.join(format!("{}_part{}.c", self.base_name, idx))
    }

    /// Path to dispatch file.
    pub fn dispatch_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}_dispatch.c", self.base_name))
    }

    /// Path to memory file.
    pub fn memory_path(&self) -> PathBuf {
        self.output_dir.join(format!("{}_memory.c", self.base_name))
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
        let mut config = self.config.clone();
        config.entry_point = X::from_u64(self.entry_point);
        config.pc_end = X::from_u64(self.pc_end);
        for &addr in &self.valid_addresses {
            config.valid_addresses.insert(addr);
        }

        let header_cfg = HeaderConfig::new(
            &self.base_name,
            &config,
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
    pub fn write_partition(
        &self,
        partition_idx: usize,
        blocks: &[&BlockIR<X>],
    ) -> std::io::Result<()> {
        let mut config = self.config.clone();
        for &addr in &self.valid_addresses {
            config.valid_addresses.insert(addr);
        }

        let mut emitter = CEmitter::new(config);
        let mut content = String::new();

        content.push_str(&format!("#include \"{}_blocks.h\"\n\n", self.base_name));

        for block in blocks {
            emitter.reset();
            emitter.render_block(block);
            content.push_str(emitter.output());
        }

        fs::write(self.partition_path(partition_idx), content)
    }

    /// Write all partition files.
    pub fn write_partitions(&self, blocks: &[BlockIR<X>]) -> std::io::Result<usize> {
        let partitions = self.partition_blocks(blocks);
        let num_partitions = partitions.len();

        for (idx, partition_blocks) in partitions {
            self.write_partition(idx, &partition_blocks)?;
        }

        Ok(num_partitions)
    }

    /// Write dispatch file.
    pub fn write_dispatch(&self) -> std::io::Result<()> {
        let mut config = self.config.clone();
        for &addr in &self.valid_addresses {
            config.valid_addresses.insert(addr);
        }

        let dispatch_cfg = DispatchConfig::new(
            &config,
            &self.base_name,
            self.entry_point,
            self.pc_end,
            self.initial_brk,
            self.valid_addresses.clone(),
            self.absorbed_to_merged.clone(),
        );

        let dispatch = gen_dispatch_file::<X>(&dispatch_cfg);
        fs::write(self.dispatch_path(), dispatch)
    }

    /// Write memory file.
    pub fn write_memory(&self) -> std::io::Result<()> {
        let mem_cfg = MemoryConfig::new(
            &self.base_name,
            self.segments.clone(),
            self.config.memory_bits,
        );

        let memory = gen_memory_file(&mem_cfg);
        fs::write(self.memory_path(), memory)
    }

    /// Write Makefile.
    pub fn write_makefile(&self, num_partitions: usize) -> std::io::Result<()> {
        let mut content = String::new();

        writeln!(content, "# Generated by RVR").unwrap();
        writeln!(content, "CC = {}", self.compiler).unwrap();

        let mut cflags = "-O3 -Wall -Wno-unused-function".to_string();
        if self.enable_lto {
            cflags.push_str(" -flto");
        }

        writeln!(content, "CFLAGS = {}", cflags).unwrap();
        writeln!(content, "SHARED_FLAGS = -fPIC").unwrap();
        writeln!(content).unwrap();

        // Source files
        let mut srcs: Vec<String> = (0..num_partitions)
            .map(|i| format!("{}_part{}.c", self.base_name, i))
            .collect();
        srcs.push(format!("{}_dispatch.c", self.base_name));
        if !self.segments.is_empty() {
            srcs.push(format!("{}_memory.c", self.base_name));
        }
        if self.enable_tohost {
            srcs.push(format!("{}_htif.c", self.base_name));
        }

        writeln!(content, "SRCS = {}", srcs.join(" ")).unwrap();
        writeln!(content, "OBJS = $(SRCS:.c=.o)").unwrap();
        writeln!(content).unwrap();

        // Targets
        writeln!(content, "shared: lib{}.so", self.base_name).unwrap();
        writeln!(content).unwrap();

        writeln!(content, "lib{}.so: $(OBJS)", self.base_name).unwrap();
        writeln!(content, "\t$(CC) $(CFLAGS) -shared -o $@ $(OBJS)").unwrap();
        writeln!(content).unwrap();

        writeln!(content, "%.o: %.c {}.h {}_blocks.h", self.base_name, self.base_name).unwrap();
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
        let block_addresses: Vec<u64> = blocks
            .iter()
            .map(|b| X::to_u64(b.start_pc))
            .collect();

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
        assert_eq!(project.blocks_header_path().to_str().unwrap(), "/tmp/test/rv64_blocks.h");
        assert_eq!(project.partition_path(0).to_str().unwrap(), "/tmp/test/rv64_part0.c");
        assert_eq!(project.dispatch_path().to_str().unwrap(), "/tmp/test/rv64_dispatch.c");
        assert_eq!(project.makefile_path().to_str().unwrap(), "/tmp/test/Makefile");
    }

    #[test]
    fn test_partition_blocks() {
        let config = EmitConfig::<Rv64>::default();
        let project = CProject::new("/tmp/test", "rv64", config)
            .with_partition_size(10);

        // Create dummy blocks with different instruction counts
        let blocks: Vec<BlockIR<Rv64>> = vec![
            create_dummy_block(0x1000, 5),  // 5 instructions
            create_dummy_block(0x2000, 3),  // 3 instructions -> partition 0 (8 total)
            create_dummy_block(0x3000, 4),  // 4 instructions -> partition 1 (4 total)
            create_dummy_block(0x4000, 6),  // 6 instructions -> partition 1 (10 total)
            create_dummy_block(0x5000, 2),  // 2 instructions -> partition 2
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
            let ir = InstrIR::new(pc, 4, Vec::new(), Terminator::default());
            block.push(ir);
        }
        block
    }
}
