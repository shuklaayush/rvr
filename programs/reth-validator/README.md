# reth-validator

RISC-V guest program for benchmarking Ethereum block validation using reth.

## Prerequisites

- Rust nightly-2025-12-05 (specified in `rust-toolchain.toml`)
- RISC-V cross-compilation toolchain (for linking)

## Building

Build all targets:

```bash
make all
```

Build specific targets:

```bash
make rv32   # RV32I (32 registers)
make rv32e  # RV32E (16 registers)
make rv64   # RV64I (32 registers)
make rv64e  # RV64E (16 registers)
make host   # Native baseline
```

Output binaries are placed in:
- `bin/reth/rv32i/reth-validator`
- `bin/reth/rv32e/reth-validator`
- `bin/reth/rv64i/reth-validator`
- `bin/reth/rv64e/reth-validator`

## Running with RVR

1. Build the guest program:
   ```bash
   make rv64  # or any other target
   ```

2. Compile to native with RVR:
   ```bash
   rvr compile bin/reth/rv64i/reth-validator -o target/rv64i/reth
   ```

3. Run the compiled program:
   ```bash
   rvr run target/rv64i/reth
   ```

## Fixtures

The `fixtures/` directory contains JSON files with Ethereum block data for validation:
- `22974575.json` - Block 22974575
- `22974576.json` - Block 22974576

## Notes

- This crate is intentionally excluded from the main Cargo workspace to avoid heavy dependencies in normal builds.
- The program uses `ecall` with `a7=93` (SYS_EXIT) to exit.
- Memory layout and entry point are defined in `link.x`.
