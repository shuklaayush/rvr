# reth

RISC-V guest program for benchmarking Ethereum block validation using reth.

## Prerequisites

- Rust nightly (see `rust-toolchain.toml`)
- RISC-V cross-compilation toolchain (for linking)

## Building

Use the `rvr bench build` command:

```bash
# Build for default architecture (rv64i)
rvr bench build reth

# Build for specific architectures
rvr bench build reth --arch rv32i
rvr bench build reth --arch rv64i,rv64e
```

Output binaries are placed in:
- `bin/rv32i/reth`
- `bin/rv32e/reth`
- `bin/rv64i/reth`
- `bin/rv64e/reth`

## Running with RVR

Use the `rvr bench run` command:

```bash
# Run benchmark
rvr bench run reth

# Compare with native host
rvr bench run reth --compare-host
```

Or manually:

1. Compile to native with RVR:
   ```bash
   rvr compile bin/rv64i/reth -o target/rv64i/reth
   ```

2. Run the compiled program:
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
