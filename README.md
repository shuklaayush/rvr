# rvr

**rvr** (RISC-V Recompiler) is a static recompiler that translates RISC-V ELF binaries to native code via C or direct assembly.

```
ELF → Lifter → IR → CFG → Emitter → C/.s → Native (.so)
```

The **lifter** decodes RISC-V instructions into a typed IR with a modular extension system (RV32/64IMAC, Zb*, Zicsr, Zicond). The **emitter** generates C or assembly with tail-call dispatch, passing hot registers as function arguments. The CFG stage sits between IR and the emitter for block structure and analysis. Since the output is native code, you can profile with standard tools (perf, Instruments) and identify hotspots at the basic block level.

The **tracer** is a pluggable instrumentation layer that hooks into execution. Provide a C header implementing the interface, and rvr inlines your callbacks at each state access:

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

This is useful for recording execution traces for replay, logging state accesses, or collecting statistics.

## Usage

```bash
# Run tests
cargo test

# Run heavy test suites with nextest (defaults limit riscv-tests/arch-tests to 5 threads)
cargo nextest run -p rvr --test riscv_tests
cargo nextest run -p rvr --test arch_tests

# Force rebuild of bin/riscv-tests and bin/riscv-arch-test before tests
RVR_REBUILD_ELFS=1 cargo test -p rvr

# Compile ELF to native shared library
rvr compile program.elf -o output/

# Run compiled program
rvr run output/ program.elf

# With Linux syscall emulation
rvr compile program.elf -o output/ --syscalls linux

# With custom tracer
rvr compile program.elf -o output/ --tracer-header my_tracer.h

# Lift to C source only
rvr lift program.elf -o output/

#
# Development benchmarks
cargo bench -p rvr --bench riscv_benchmarks

# Backend selection
cargo run -- compile program.elf --backend c      # C (default)
cargo run -- compile program.elf --backend x86    # x86-64 assembly
cargo run -- compile program.elf --backend arm64  # ARM64 assembly
```

## GDB

`rvr run` can host a GDB remote stub for interactive debugging:

```bash
# Compile first
rvr compile bin/rv64i/minimal -o /tmp/minimal

# Terminal 1: start GDB server
rvr run --gdb :1234 /tmp/minimal bin/rv64i/minimal

# Terminal 2: connect
riscv64-unknown-elf-gdb bin/rv64i/minimal
(gdb) target remote :1234
```

## Differential/Trace Debugging

Quick diff/trace tools for backend regression debugging:

```bash
# In-memory diff (fast)
rvr dev diff spike-c bin/riscv-tests/rv64ui-p-add --max-instrs 1000

# On-disk trace compare (slower, deeper)
rvr dev trace bin/riscv-tests/rv64ui-p-add
```

## Environment Variables

Test/bench helpers:
- `RVR_REBUILD_ELFS=1`: rebuilds `bin/riscv-tests` and `bin/riscv-arch-test` before tests.

Nextest:
- `.config/nextest.toml` assigns `rvr::riscv_tests` and `rvr::arch_tests` to a test group capped at 5 threads.

Examples:
```bash
RVR_REBUILD_ELFS=1 cargo test -p rvr --test riscv_tests
RVR_REBUILD_ELFS=1 cargo test -p rvr --test arch_tests
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

## Directory Structure

```
bin/
├── host/           # Host binaries for comparisons
├── rv32e/          # RV32E binaries
├── rv32i/          # RV32I binaries
│   └── reth
├── rv64e/          # RV64E binaries
├── rv64i/          # RV64I binaries
│   ├── coremark
│   ├── dhrystone
│   ├── minimal
│   ├── pinky
│   ├── prime-sieve
│   ├── reth
│   └── ...
└── riscv-tests/    # riscv-tests suite
    ├── rv32ui-p-add
    ├── rv64ui-p-add
    └── ...
```

## Crates

| Crate | Description |
|-------|-------------|
| `rvr` | CLI and high-level API |
| `rvr-cfg` | Control flow graph |
| `rvr-ir` | Intermediate representation |
| `rvr-isa` | Decoder, lifter, extensions |
| `rvr-emit` | Code generation (C, x86-64, ARM64) |
| `rvr-elf` | ELF parsing |
| `rvr-state` | Runtime state definitions |
| `rvr-rt` | Runtime support |
