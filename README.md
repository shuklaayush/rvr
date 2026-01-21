# rvr

A fast RISC-V static recompiler. Translates ELF binaries to native code via C.

```
ELF → Lifter → IR → Emitter → C → Native (.so)
```

## Design

**Lifter**: Decodes RISC-V instructions into a typed IR. Modular extension system (RV32/64IMAC, Zb*, Zicsr, Zicond) with traits for custom instructions.

**IR**: Platform-agnostic representation—statements, expressions, and terminators that capture all guest behavior.

**Emitter**: Generates C code with tail-call dispatch. Hot registers are passed as function arguments to minimize memory access.

**Tracer**: Pluggable instrumentation via C headers. Implement hooks to observe execution:

```c
// Required tracer interface
typedef struct Tracer { /* your state */ } Tracer;

static inline void trace_init(Tracer* t);
static inline void trace_pc(Tracer* t, uint64_t pc, uint16_t op);
static inline void trace_reg_read(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value);
static inline void trace_reg_write(Tracer* t, uint64_t pc, uint16_t op, uint8_t reg, uint64_t value);
static inline void trace_mem_read_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value);
static inline void trace_mem_write_word(Tracer* t, uint64_t pc, uint16_t op, uint64_t addr, uint32_t value);
static inline void trace_branch_taken(Tracer* t, uint64_t pc, uint16_t op, uint64_t target);
// ... additional hooks for memory sizes, CSRs, etc.
```

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

# Lift to C source only (no compilation)
rvr lift program.elf -o output/

# Run riscv-tests
rvr test riscv run --filter rv64ui --verbose
```

## Syscalls

Two modes: `baremetal` (exit only) and `linux` (full emulation). Custom syscalls via the `SyscallTable` API:

```rust
let table = SyscallTable::new(SyscallAbi::Standard)
    .with_exit(93)
    .with_runtime(64, "rv_sys_write", 3)
    .with_runtime(555, "my_handler", 2);  // custom

let registry = ExtensionRegistry::<Rv64>::standard()
    .with_syscall_handler(table);
```

Then implement `my_handler` in C and link it.

## Crates

| Crate | Description |
|-------|-------------|
| `rvr` | CLI and high-level API |
| `rvr-ir` | Intermediate representation |
| `rvr-isa` | Decoder, lifter, extensions |
| `rvr-cfg` | Control flow graph |
| `rvr-emit` | C code generation |
| `rvr-elf` | ELF parsing |
