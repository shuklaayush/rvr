# Custom tracers

This directory contains example C tracer headers that can be passed to
`TracerConfig::custom_file(...)`.

The generated C expects a header named `rv_tracer.h` in the output directory.
When using `custom_file`, its contents are copied into `rv_tracer.h`.

Tips
- Start with `minimal.h` and add functions as needed.
- Keep the function signatures identical to the built-in tracers.
- If you want a Rust tracer for experiments, implement it in Rust and
  generate an equivalent C header when you need the fast inline path.

Rust-side example
- `rust/pc_count.rs` shows a minimal Rust tracer for analysis.
- `scripts/emit_tracer_header.py` emits a C header skeleton from the Rust tracer file.

Example
`./scripts/emit_tracer_header.py crates/rvr-emit/tracers/rust/pc_count.rs /tmp/pc_count.h --xlen 64`
