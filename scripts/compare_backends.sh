#!/bin/bash
# Compare ARM64 and C backend state for a benchmark
# Usage: ./scripts/compare_backends.sh coremark

BENCH="${1:-coremark}"
RUNS=1

echo "=== Comparing backends for $BENCH ==="

# Compile with C backend
echo "Compiling with C backend..."
cargo run --release -- bench compile "$BENCH" --backend c --force 2>/dev/null

# Compile with ARM64 backend
echo "Compiling with ARM64 backend..."
cargo run --release -- bench compile "$BENCH" --backend arm64 --force 2>/dev/null

# Run C backend and capture state
echo ""
echo "=== C Backend ==="
RUST_LOG=off cargo run --release -- bench run "$BENCH" --backend c --runs $RUNS 2>&1 | grep -E "^(x[0-9]|pc:|instret|crc)" | head -40

echo ""
echo "=== ARM64 Backend ==="
RUST_LOG=off cargo run --release -- bench run "$BENCH" --backend arm64 --runs $RUNS 2>&1 | grep -E "^(x[0-9]|pc:|instret|crc)" | head -40
