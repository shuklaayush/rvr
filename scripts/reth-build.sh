#!/bin/bash
# Build reth-validator guest programs for RISC-V targets
#
# Usage:
#   ./scripts/reth-build.sh [target...]
#
# Examples:
#   ./scripts/reth-build.sh          # Build all targets
#   ./scripts/reth-build.sh rv64     # Build RV64I only
#   ./scripts/reth-build.sh rv32 rv64  # Build RV32I and RV64I

set -e

cd "$(dirname "$0")/.."
RETH_DIR="programs/reth-validator"

if [ ! -d "$RETH_DIR" ]; then
    echo "Error: $RETH_DIR not found"
    exit 1
fi

# Default to all targets if none specified
TARGETS="${@:-all}"

echo "Building reth-validator guest programs..."
echo "Targets: $TARGETS"
echo

make -C "$RETH_DIR" $TARGETS

echo
echo "Build complete. Output binaries:"
for arch in rv32i rv32e rv64i rv64e; do
    bin="bin/reth/$arch/reth-validator"
    if [ -f "$bin" ]; then
        size=$(stat -c%s "$bin" 2>/dev/null || stat -f%z "$bin" 2>/dev/null)
        echo "  $bin ($size bytes)"
    fi
done
