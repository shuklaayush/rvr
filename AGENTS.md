# rvr Development Guide

This repository contains **rvr** (RISC-V Recompiler), a static recompiler that translates RISC-V ELF binaries to native code via C or direct assembly.

## Quick Start

```bash
cargo build                    # Build all crates
cargo build --release          # Release build
cargo test                     # Run all tests
cargo fmt                      # Format code
cargo clippy                   # Lint code
```

## CLI Usage

```bash
# Compile ELF to native shared library
rvr compile program.elf -o output/

# Run compiled program
rvr run output/ program.elf

# With Linux syscall emulation
rvr compile program.elf -o output/ --syscalls linux

# With custom tracer
rvr compile program.elf -o output/ --tracer-header my_tracer.h

# Lift to C source only (no compilation)
rvr lift program.elf -o output/

# Run riscv-tests
rvr test riscv build                # build from source (requires riscv toolchain)
rvr test riscv run                  # run all tests
rvr test riscv run --filter rv64ui  # filtered

# Run riscv-arch-test (official architecture compliance tests)
rvr test arch build                 # build from source (requires riscv toolchain)
rvr test arch build --category rv64i-I  # build specific category
rvr test arch gen-refs              # generate reference signatures (requires Spike)
rvr test arch run                   # run all tests
rvr test arch run --filter add      # filtered

# Benchmarks
rvr bench list                       # List available benchmarks
rvr bench build                      # Build all from source
rvr bench compile                    # Compile all to native
rvr bench run                        # Run all benchmarks

# Single benchmark
rvr bench build reth                 # Build reth ELF + host binary
rvr bench compile reth               # Compile to native
rvr bench run reth --compare-host    # Run with host comparison
```

## Repository Structure

```text
rvr/
├── crates/
│   ├── rvr/           # CLI and high-level API
│   │   └── src/commands/
│   │       ├── test/          # Test suite commands
│   │       │   ├── riscv_tests/   # riscv-tests runner
│   │       │   └── arch_tests/    # riscv-arch-test runner (includes harness files)
│   │       └── bench/         # Benchmark commands
│   │           └── coremark/  # CoreMark port files
│   ├── rvr-elf/       # ELF parsing
│   ├── rvr-isa/       # Decoder, lifter, extensions
│   ├── rvr-ir/        # Intermediate representation
│   ├── rvr-cfg/       # Control flow graph
│   ├── rvr-emit/      # Code generation (C, x86-64, ARM64)
│   ├── rvr-state/     # Runtime state definitions
│   └── rvr-rt/        # Runtime support
├── bin/               # Pre-built binaries (Git LFS)
│   ├── host/          # Host binaries for comparisons
│   ├── rv32e/         # RV32E binaries
│   ├── rv32i/         # RV32I binaries
│   ├── rv64e/         # RV64E binaries
│   ├── rv64i/         # RV64I binaries
│   ├── riscv-tests/   # riscv-tests suite (rv32ui-p-*, rv64ui-p-*, etc.)
│   └── riscv-arch-test/  # riscv-arch-test binaries and references
├── programs/          # Source for test programs and submodules
│   ├── riscv-tests/       # Submodule: upstream riscv-tests
│   └── riscv-arch-test/   # Submodule: upstream riscv-arch-test
├── docs/              # Design notes and backend TODOs
└── scripts/           # Helper scripts
```

## Pipeline

```
ELF → Lifter → IR → CFG → Emitter → C/.s → Native (.so)
```

The **lifter** decodes RISC-V instructions into a typed IR with a modular extension system (RV32/64IMAC, Zb*, Zicsr, Zicond). The **emitter** generates C or assembly with tail-call dispatch, passing hot registers as function arguments. The CFG stage (`rvr-cfg`) sits between IR and the emitter for block structure and analysis. Since the output is native code, you can profile with standard tools (perf, Instruments) and identify hotspots at the basic block level.

## Development Guidelines

### Design Principles

- **Do The Work**: Don't be lazy. Large architectural changes are fine - do them properly with good taste. Design clean, elegant, idiomatic Rust solutions.
- **Design First**: Before implementing, explore the codebase thoroughly and design a clean, elegant approach. Write out detailed step-by-step todos with small feedback loops. Each step should be testable.
- **Single Source of Truth**: Derive values from canonical sources rather than duplicating. For example, derive struct sizes from XLEN rather than hardcoding separate RV32/RV64 values. Derive XLEN from ELF rather than requiring a flag.
- **Avoid Redundant Parameters**: If a value can be derived from another input (like XLEN from ELF), don't add a separate flag.
- **Question Before Optimizing**: Don't optimize complexity that shouldn't exist. Before adding prefixes like `rv32_`/`rv64_` or separate types, ask: can this be unified? A single generic type is better than duplicated code paths.
- **No Bloat**: KISS principle. Only create abstractions when there's actual logic or variation.
- **No Magic Constants**: Use `const` with descriptive names.
- **No Constant Duplication**: For shared addresses like the HTIF `tohost` location, define a single `TOHOST_ADDR` constant in a shared location and reuse it instead of re-declaring the value across modules.
- **No Warnings**: Code must compile without warnings. Use `#![deny(warnings)]` in crate roots.
- **Delete Tech Debt**: Remove unused code immediately.
- **Script Large Refactors**: For repetitive refactors, prefer writing a script over manual brute-force edits. Always commit or stash work before running scripts.

### Repository Maintenance

- **LFS for Binaries**: All binaries in `bin/` are tracked with Git LFS.
- **Keep Dependencies Minimal**: Don't add dependencies for trivial functionality.
- **Document Public APIs**: All public items need doc comments.

### Submodule Management

Submodules in `programs/` contain upstream test suites and benchmarks. **Never modify submodules** for rvr-specific functionality - only update them to track upstream changes or fix upstream bugs.

**Pattern for test/benchmark harnesses**: Keep rvr-specific harness files (linker scripts, model headers, port files) alongside the Rust code that uses them in `crates/rvr/src/commands/`:

```text
crates/rvr/src/commands/
├── test/
│   ├── riscv_tests/          # riscv-tests runner code
│   └── arch_tests/           # riscv-arch-test runner code
│       ├── mod.rs            # Test runner implementation
│       ├── model_test.h      # RVMODEL_* macros for rvr target
│       └── link.ld           # Linker script for rvr
└── bench/
    └── coremark/             # CoreMark benchmark
        ├── mod.rs            # Build/run implementation
        ├── host_portme.h     # Host port header
        ├── host_portme.c     # Host port implementation
        ├── riscv_portme.h    # RISC-V port header
        └── riscv_portme.c    # RISC-V port implementation
```

This pattern keeps harness files with the code that uses them, making it clear which files belong together. The upstream submodules remain unmodified.

### Development Workflow

**Maintain a Todo List**: Keep a detailed plan broken down into small, testable steps that can be verified one by one. Each step should have clear success criteria. Update the todo list as you work.

**Test Small First**: Before running full test suites or benchmarks that take minutes, verify with small quick tests. A single riscv-test file runs in seconds vs minutes for full suites.

**Run Benchmarks Individually**: When testing benchmark functionality (especially with new backends), run benchmarks one at a time with short timeouts (30-60s). Never run `bench run` without a specific benchmark name when debugging - a single hanging benchmark will block indefinitely. HTIF-using benchmarks (towers, median, dhrystone, coremark) are particularly prone to hanging when the HTIF handler isn't working correctly.

**Small Closed Feedback Loops**: When working on bugs:
1. Use test cases that fail fast and give clear error messages
2. Decode error signals: riscv-tests exit code = `(test_case << 1) | 1`, so exit code 11 means test case 5
3. Disassemble test binaries to understand what instruction is being tested
4. Look at working implementations as templates

**Optimization Discipline**: For performance work, keep changes small, commit each logical improvement, and run a fast validation step (single riscv-test or one benchmark) after every change before moving on.

**Conventional Commits**: Use conventional commit messages (e.g. `feat: ...`, `fix: ...`, `refactor: ...`) and keep commits small and logical.

### Rust Patterns

- **Generics Over Duplication**: Use generics and const generics to avoid duplicating code for RV32/RV64. A single `RvState<const XLEN: usize>` is better than separate `Rv32State` and `Rv64State`. But don't over-abstract - if code becomes harder to read than simple duplication, skip the generic.
- **Use the Type System**: Encode invariants in types. Use newtypes for distinct concepts (addresses, register indices, etc.).
- **Error Handling**: Use `thiserror` for error types. Prefer `Result` over panics except for truly unrecoverable errors.
- **Concise Documentation**: Doc comments should be self-contained and concise. One line preferred. The code should be self-documenting.

## RISC-V Architecture

The implementation supports both RV32 and RV64 via XLEN parameterization:

**Supported Extensions**:
- **I**: Base integer (RV32I/RV64I)
- **M**: Multiply/divide
- **A**: Atomics (LR/SC, AMO operations)
- **C**: Compressed instructions
- **Zicsr**: CSR access
- **Zicond**: Conditional operations
- **Zb\***: Bit manipulation (Zba, Zbb, Zbc, Zbs)

**RV64-specific instructions**: LD, SD, LWU, ADDIW, ADDW, SUBW, SLLIW, SRLIW, SRAIW, SLLW, SRLW, SRAW

## Testing and Benchmarks

```bash
# Run all tests
cargo test

# Run riscv-tests
cargo run -- test riscv run
cargo run -- test riscv run --filter rv64ui  # filtered

# Run benchmarks
cargo run -- bench run
```

riscv-tests binaries are in `bin/riscv-tests/` (tracked with Git LFS).

**Backend Smoke Tests**: For quick backend checks, run a single riscv-test file with a specific backend, e.g.:
```bash
./target/release/rvr test --backend arm64 bin/riscv-tests/rv64ui-p-add
```

## Code Generation Backends

### C Backend (default)

Generates portable C with tail-call dispatch. Compatible with any C compiler but requires gcc/clang for compilation.

### x86-64 Backend

Direct assembly emission targeting x86-64 with AT&T syntax. Uses aggressive register allocation for hot RISC-V registers.

### ARM64 Backend (in progress)

Direct assembly emission targeting AArch64. Optimized for ARM64's larger register file.

### Assembly Backend Notes (x86/ARM64)

- **Linear emission**: asm backends emit a single instruction stream with labels. CFG analysis can still be used for metadata (valid PCs, absorbed blocks), but emission is per-instruction.
- **IR Temps**: IR temp registers (`ReadExpr::Temp` / `WriteTarget::Temp`) are backed by fixed stack slots in asm backends. Keep spill slots separate from IR temp slots to avoid clobbering AMO/LRSC/JALR temps.
- **Reservation state**: `ResAddr`/`ResValid` reads/writes must be implemented for LR/SC and AMO correctness.
- **Address evaluation**: When an address expression is not already in the canonical address register, explicitly move it before applying address masking. This is required for `MemAddr`, `Mem` writes, and `JumpDyn`.
- **Shift lowering**: For variable shifts, evaluate the shift amount without clobbering the left operand. In ARM64 this means spilling left before evaluating right; x86 uses CL for shifts.

### Backend Selection

```bash
cargo run -- compile program.elf --backend c      # C (default)
cargo run -- compile program.elf --backend x86    # x86-64 assembly
cargo run -- compile program.elf --backend arm64  # ARM64 assembly
```

## Syscalls

Two modes: `baremetal` (exit only) and `linux` (full emulation). Custom syscalls via `SyscallTable`:

```rust
let table = SyscallTable::new(SyscallAbi::Standard)
    .with_exit(93)
    .with_runtime(64, "rv_sys_write", 3)
    .with_runtime(555, "my_handler", 2);

let registry = ExtensionRegistry::<Rv64>::standard()
    .with_syscall_handler(table);
```

## Tracer System

The tracer is a pluggable instrumentation layer. Provide a C header implementing the interface, and rvr inlines callbacks at each state access:

```c
typedef struct Tracer { /* your state */ } Tracer;

static inline void trace_pc(Tracer* t, uint64_t pc, uint16_t op);
static inline void trace_reg_read(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value);
static inline void trace_reg_write(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value);
static inline void trace_mem_read_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value);
static inline void trace_mem_write_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value);
static inline void trace_branch_taken(Tracer* t, uint64_t pc, uint16_t op, uint64_t target);
// ... hooks for other memory sizes, CSRs, etc.
```

Useful for recording execution traces for replay, logging state accesses, or collecting statistics.

## Git LFS

All binaries in `bin/` are tracked with Git LFS:

```bash
git lfs install         # Ensure LFS is installed
git lfs pull            # Pull binary files
git lfs ls-files        # List LFS-tracked files
```

**Workflow for adding new binaries**:
```bash
git add bin/rv64i/new-binary
git commit -m "chore(bin): add new binary"
# Files matching bin/** patterns are automatically tracked via .gitattributes
```
