# rvr

**rvr** (RISC-V Recompiler) is a static recompiler that translates RISC-V ELF binaries to native code via C.

```
ELF → Lifter → IR → Emitter → C → Native (.so)
```

The **lifter** decodes RISC-V instructions into a typed IR with a modular extension system (RV32/64IMAC, Zb*, Zicsr, Zicond). The **emitter** generates C with tail-call dispatch, passing hot registers as function arguments. Since the output is native code, you can profile with standard tools (perf, Instruments) and identify hotspots at the basic block level.

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
rvr test riscv run              # all tests
rvr test riscv run --filter rv64ui  # filtered

# Reth benchmark
rvr bench reth compile --arch rv64i
rvr bench reth run --arch rv64i
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

## Crates

| Crate | Description |
|-------|-------------|
| `rvr` | CLI and high-level API |
| `rvr-ir` | Intermediate representation |
| `rvr-isa` | Decoder, lifter, extensions |
| `rvr-cfg` | Control flow graph |
| `rvr-emit` | C code generation |
| `rvr-elf` | ELF parsing |
