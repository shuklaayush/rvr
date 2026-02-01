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

# Run riscv-tests
rvr test riscv build                # build from source (requires riscv toolchain)
rvr test riscv run                  # run all tests
rvr test riscv run --filter rv64ui  # filtered

# Benchmarks
rvr bench list                       # List available benchmarks
rvr bench build                      # Build all from source
rvr bench compile                    # Compile all to native
rvr bench run                        # Run all benchmarks

# Single benchmark
rvr bench build reth                 # Build reth ELF + host binary
rvr bench compile reth               # Compile to native
rvr bench run reth --compare-host    # Run with host comparison

# Backend selection
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
