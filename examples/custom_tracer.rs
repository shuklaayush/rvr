//! Custom tracer example.
//!
//! Demonstrates supplying a custom tracer header inline and wiring it into
//! the pipeline. The tracer prints a small summary at the end of execution.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example custom_tracer -- path/to/program.elf output_dir/
//! ```

use std::path::PathBuf;
use std::process::Command;

use rvr::{EmitConfig, Pipeline, Runner, Rv64, TracerConfig};
use rvr_isa::ExtensionRegistry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (elf_path, output_dir) = parse_args()?;

    // Minimal tracer that counts instructions and register accesses.
    let tracer_header = r#"/* Minimal custom tracer */
#pragma once

#include <stdint.h>
#include <stdio.h>

typedef struct Tracer {
    uint64_t pcs;
    uint64_t reg_accesses;
} Tracer;

static inline void trace_init(Tracer* t) {
    t->pcs = 0;
    t->reg_accesses = 0;
}

static inline void trace_fini(Tracer* t) {
    printf("trace: pcs=%lu reg_accesses=%lu\n", t->pcs, t->reg_accesses);
}

static inline void trace_block(Tracer* t, uint64_t pc) { (void)t; (void)pc; }
static inline void trace_pc(Tracer* t, uint64_t pc, uint16_t op) {
    (void)pc;
    (void)op;
    t->pcs++;
}
static inline void trace_opcode(Tracer* t, uint64_t pc, uint16_t op, uint32_t opcode) {
    (void)t;
    (void)pc;
    (void)op;
    (void)opcode;
}

static inline void trace_reg_read(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value) {
    (void)pc;
    (void)op;
    (void)reg;
    (void)value;
    t->reg_accesses++;
}

static inline void trace_reg_write(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value) {
    (void)pc;
    (void)op;
    (void)reg;
    (void)value;
    t->reg_accesses++;
}

static inline void trace_mem_read_byte(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint8_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}
static inline void trace_mem_read_halfword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint16_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}
static inline void trace_mem_read_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}
static inline void trace_mem_read_dword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint64_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}
static inline void trace_mem_write_byte(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint8_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}
static inline void trace_mem_write_halfword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint16_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}
static inline void trace_mem_write_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}
static inline void trace_mem_write_dword(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint64_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)addr;
    (void)value;
}

static inline void trace_branch_taken(Tracer* t, uint64_t pc, uint16_t op, uint64_t target) {
    (void)t;
    (void)pc;
    (void)op;
    (void)target;
}
static inline void trace_branch_not_taken(Tracer* t, uint64_t pc, uint16_t op, uint64_t target) {
    (void)t;
    (void)pc;
    (void)op;
    (void)target;
}

static inline void trace_csr_read(Tracer* t, uint64_t pc, uint16_t op, uint16_t csr, uint64_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)csr;
    (void)value;
}
static inline void trace_csr_write(Tracer* t, uint64_t pc, uint16_t op, uint16_t csr, uint64_t value) {
    (void)t;
    (void)pc;
    (void)op;
    (void)csr;
    (void)value;
}
"#;

    let mut config = EmitConfig::<Rv64>::default();
    config.tracer_config = TracerConfig::custom_inline("mini", tracer_header, Vec::new());

    let data = std::fs::read(&elf_path)?;
    let image = rvr::ElfImage::<Rv64>::parse(&data)?;
    let registry = ExtensionRegistry::<Rv64>::standard();
    let mut pipeline = Pipeline::with_registry(image, config, registry);

    pipeline.build_cfg()?;
    pipeline.lift_to_ir()?;

    std::fs::create_dir_all(&output_dir)?;
    let base_name = output_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rv");
    pipeline.emit_c(&output_dir, base_name)?;

    let status = Command::new("make")
        .arg("-C")
        .arg(&output_dir)
        .arg("shared")
        .status()?;
    if !status.success() {
        return Err("make failed".into());
    }

    let mut runner = Runner::load(&output_dir, &elf_path)?;
    let result = runner.run()?;
    println!("Exit code: {}", result.exit_code);
    println!("Instructions: {}", result.instret);

    Ok(())
}

fn parse_args() -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <elf_path> <output_dir>", args[0]);
        std::process::exit(1);
    }
    Ok((PathBuf::from(&args[1]), PathBuf::from(&args[2])))
}
