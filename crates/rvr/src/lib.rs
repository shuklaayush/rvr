//! RVR - RISC-V Static Recompiler
//!
//! Compiles RISC-V ELF binaries to optimized C code, then to native shared libraries.
//!
//! # Quick Start
//!
//! ```ignore
//! // Compile an ELF to a shared library (auto-detects RV32/RV64)
//! let lib_path = rvr::compile("program.elf".as_ref(), "output/".as_ref())?;
//! ```
//!
//! # Architecture
//!
//! RVR generates `.so` shared libraries that can be loaded by a host runtime.
//! The generated code:
//!
//! - Uses `preserve_none` calling convention for minimal overhead
//! - Passes hot registers as function arguments (configurable)
//! - Uses `[[clang::musttail]]` for guaranteed tail call optimization
//! - Generates C23 code with `constexpr`, typed constants, no macros
//!
//! ## Generated Interface
//!
//! The shared library exports:
//!
//! ```c
//! // Execute from a specific PC (returns exit code)
//! int rv_execute_from(RvState* state, uint32_t start_pc);
//!
//! // Initialize memory with embedded ELF segments
//! void rv_init_memory(RvState* state);
//!
//! // Free memory
//! void rv_free_memory(RvState* state);
//!
//! // Dispatch table for dynamic jumps
//! extern const rv_fn dispatch_table[];
//! ```
//!
//! ## State Structure
//!
//! The `RvState` struct is defined in the generated header. Key fields:
//!
//! - `memory`: Pointer to guest memory (allocated by host)
//! - `regs[32]`: General-purpose registers
//! - `pc`: Program counter
//! - `instret`: Retired instruction count (if enabled)
//! - `has_exited`, `exit_code`: Execution status
//!
//! # Examples
//!
//! ## Basic Compilation
//!
//! ```ignore
//! use rvr::{compile, CompileOptions, AddressMode, InstretMode};
//!
//! // Simple compilation
//! let lib = rvr::compile("prog.elf".as_ref(), "out/".as_ref())?;
//!
//! // With options
//! let options = CompileOptions::new()
//!     .with_instret_mode(InstretMode::Count)
//!     .with_address_mode(AddressMode::Bounds);
//! let lib = rvr::compile_with_options("prog.elf".as_ref(), "out/".as_ref(), &options)?;
//! ```
//!
//! ## Custom Configuration
//!
//! ```ignore
//! use rvr::{EmitConfig, Recompiler, Rv64};
//!
//! let mut config = EmitConfig::<Rv64>::default();
//! config.hot_regs = vec![1, 2, 10, 11, 12]; // ra, sp, a0, a1, a2
//! config.memory_bits = 32; // 4GB address space
//! config.enable_lto = true;
//!
//! let recompiler = Recompiler::new(config);
//! let lib = recompiler.compile("prog.elf".as_ref(), "out/".as_ref(), 0)?;
//! ```
//!
//! ## Instruction Overrides
//!
//! Customize instruction behavior (e.g., ECALL handling):
//!
//! ```ignore
//! use rvr::{ElfImage, EmitConfig, Pipeline, Rv64};
//! use rvr_isa::{ExtensionRegistry, InstructionOverride, OP_ECALL, DecodedInstr};
//! use rvr_ir::{InstrIR, Terminator, Expr};
//!
//! struct MyEcallHandler;
//!
//! impl InstructionOverride<Rv64> for MyEcallHandler {
//!     fn lift(
//!         &self,
//!         instr: &DecodedInstr<Rv64>,
//!         _default: &dyn Fn(&DecodedInstr<Rv64>) -> InstrIR<Rv64>,
//!     ) -> InstrIR<Rv64> {
//!         // Custom ECALL: exit with a0 as code
//!         InstrIR::new(
//!             instr.pc, instr.size, instr.opid.pack(),
//!             Vec::new(),
//!             Terminator::exit(Expr::read(10)), // a0
//!         )
//!     }
//! }
//!
//! let registry = ExtensionRegistry::<Rv64>::standard()
//!     .with_override(OP_ECALL, MyEcallHandler);
//!
//! let data = std::fs::read("prog.elf")?;
//! let image = ElfImage::<Rv64>::parse(&data)?;
//! let mut pipeline = Pipeline::with_registry(image, EmitConfig::default(), registry);
//! ```
//!
//! ## Extension Registry (Builder Pattern)
//!
//! Enable only the RISC-V extensions you need:
//!
//! ```ignore
//! use rvr_isa::ExtensionRegistry;
//! use rvr::Rv64;
//!
//! // Start with base I extension, add only what you need
//! let registry = ExtensionRegistry::<Rv64>::base()
//!     .with_m()      // Integer multiply/divide
//!     .with_a()      // Atomics
//!     .with_c()      // Compressed (16-bit) instructions
//!     .with_zicsr(); // CSR access
//!
//! // Or use standard() for all common extensions
//! let full = ExtensionRegistry::<Rv64>::standard();
//!
//! // Typical Linux userspace configuration
//! let linux = ExtensionRegistry::<Rv64>::base()
//!     .with_c()       // Compressed first (for correct decode order)
//!     .with_m()       // Multiply/divide
//!     .with_a()       // Atomics
//!     .with_zicsr()   // CSR access
//!     .with_zba()     // Address generation
//!     .with_zbb();    // Basic bit manipulation
//! ```
//!
//! Available extensions:
//! - `with_m()` - Integer multiply/divide (M)
//! - `with_a()` - Atomics (A)
//! - `with_c()` - Compressed 16-bit instructions (C) - add first
//! - `with_zicsr()` - CSR read/write
//! - `with_zifencei()` - Instruction fence
//! - `with_zba()` - Address generation (Zba)
//! - `with_zbb()` - Basic bit manipulation (Zbb)
//! - `with_zbs()` - Single-bit operations (Zbs)
//! - `with_zbkb()` - Bitmanip for crypto (Zbkb)
//! - `with_zicond()` - Conditional operations (Zicond)
//!
//! ## Pipeline API (Low-Level)
//!
//! For fine-grained control over the compilation process:
//!
//! ```ignore
//! use rvr::{ElfImage, EmitConfig, Pipeline, Rv64};
//!
//! let data = std::fs::read("prog.elf")?;
//! let image = ElfImage::<Rv64>::parse(&data)?;
//!
//! let mut pipeline = Pipeline::new(image, EmitConfig::default());
//!
//! // Build CFG (decode, analyze, optimize)
//! pipeline.build_cfg()?;
//! println!("Blocks: {:?}", pipeline.stats());
//!
//! // Lift to IR
//! pipeline.lift_to_ir()?;
//!
//! // Inspect IR blocks
//! for (pc, block) in pipeline.ir_blocks() {
//!     println!("Block at {:#x}: {} instructions", pc, block.instructions.len());
//! }
//!
//! // Emit C code
//! pipeline.emit_c("out/".as_ref(), "prog")?;
//! ```
//!
//! # Crate Structure
//!
//! - `rvr` - High-level API (this crate)
//! - `rvr_elf` - ELF parsing
//! - `rvr_isa` - RISC-V instruction definitions, decoder, extension registry
//! - `rvr_ir` - Intermediate representation
//! - `rvr_cfg` - Control flow graph analysis
//! - `rvr_emit` - C code generation
//!
//! # Feature Flags
//!
//! Currently no optional Cargo features. RISC-V extensions are selected at
//! runtime via the `ExtensionRegistry` builder pattern (see above). The default
//! `Pipeline::new()` uses `ExtensionRegistry::standard()` which enables all
//! common extensions (I, M, A, C, Zicsr, Zifencei, Zba, Zbb, Zbs, Zbkb, Zicond).

// Core types - always available
pub use rvr_elf::{ElfImage, get_elf_xlen};
pub use rvr_emit::c::TracerConfig;
pub use rvr_emit::{
    AddressMode, AnalysisMode, Backend, Compiler, EmitConfig, FixedAddressConfig, InstretMode,
    SyscallMode,
};
pub use rvr_isa::{Rv32, Rv64, Xlen};

// CSR constants for use with Runner::get_csr/set_csr
pub use rvr_isa::extensions::{CSR_CYCLE, CSR_INSTRET, CSR_TIME};

pub mod perf;
mod pipeline;
pub use pipeline::{Pipeline, PipelineStats};

mod runner;
pub use runner::{PerfCounters, RunError, RunResult, RunResultWithPerf, Runner};

pub mod bench;
pub mod build_utils;
mod compile;
mod error;
pub mod gdb;
pub mod metrics;
mod recompiler;
pub mod test_support;

pub use compile::{
    CompileOptions, compile, compile_with_options, lift_to_c, lift_to_c_with_options,
};
pub use error::{Error, Result};
pub use recompiler::Recompiler;

#[cfg(test)]
mod tests;
