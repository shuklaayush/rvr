#!/usr/bin/env bash
# Build riscv-tests and copy binaries to bin/riscv/tests/
#
# Prerequisites:
#   - RISC-V GCC toolchain (riscv64-unknown-elf-gcc)
#   - autoconf, automake
#
# Install on Fedora: sudo dnf install riscv64-elf-gcc autoconf automake
# Install on Ubuntu: sudo apt install gcc-riscv64-unknown-elf autoconf automake

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
TESTS_DIR="$ROOT_DIR/tests/riscv-tests"
OUTPUT_DIR="$ROOT_DIR/bin/riscv/tests"

# Check for RISC-V toolchain
if ! command -v riscv64-unknown-elf-gcc &> /dev/null; then
    if ! command -v riscv64-elf-gcc &> /dev/null; then
        echo "Error: RISC-V GCC toolchain not found"
        echo "Install with: sudo dnf install riscv64-elf-gcc (Fedora)"
        echo "         or: sudo apt install gcc-riscv64-unknown-elf (Ubuntu)"
        exit 1
    fi
    # Fedora uses riscv64-elf-* prefix
    RISCV_PREFIX="riscv64-elf-"
else
    RISCV_PREFIX="riscv64-unknown-elf-"
fi

echo "Using toolchain prefix: $RISCV_PREFIX"

# Initialize submodule if needed
if [ ! -f "$TESTS_DIR/configure" ]; then
    echo "Initializing riscv-tests submodule..."
    cd "$ROOT_DIR"
    git submodule update --init --recursive tests/riscv-tests
fi

# Build riscv-tests
echo "Building riscv-tests..."
cd "$TESTS_DIR"

# Initialize autoconf if needed
if [ ! -f "configure" ] || [ ! -f "Makefile" ]; then
    autoconf
fi

# Configure with RISC-V prefix
if [ ! -f "Makefile" ]; then
    ./configure --prefix="$TESTS_DIR/build" --with-xlen=64
fi

# Build
make -j$(nproc) 2>/dev/null || make

# Create output directories
mkdir -p "$OUTPUT_DIR/rv32ui" "$OUTPUT_DIR/rv32um" "$OUTPUT_DIR/rv32ua"
mkdir -p "$OUTPUT_DIR/rv64ui" "$OUTPUT_DIR/rv64um" "$OUTPUT_DIR/rv64ua"

# Copy binaries (strip .dump files, keep only executables)
echo "Copying binaries to $OUTPUT_DIR..."

# RV32 tests
for f in "$TESTS_DIR/isa/rv32ui-p-"*; do
    [ -f "$f" ] && [ ! "${f##*.}" = "dump" ] && cp "$f" "$OUTPUT_DIR/rv32ui/" 2>/dev/null || true
done
for f in "$TESTS_DIR/isa/rv32um-p-"*; do
    [ -f "$f" ] && [ ! "${f##*.}" = "dump" ] && cp "$f" "$OUTPUT_DIR/rv32um/" 2>/dev/null || true
done
for f in "$TESTS_DIR/isa/rv32ua-p-"*; do
    [ -f "$f" ] && [ ! "${f##*.}" = "dump" ] && cp "$f" "$OUTPUT_DIR/rv32ua/" 2>/dev/null || true
done

# RV64 tests
for f in "$TESTS_DIR/isa/rv64ui-p-"*; do
    [ -f "$f" ] && [ ! "${f##*.}" = "dump" ] && cp "$f" "$OUTPUT_DIR/rv64ui/" 2>/dev/null || true
done
for f in "$TESTS_DIR/isa/rv64um-p-"*; do
    [ -f "$f" ] && [ ! "${f##*.}" = "dump" ] && cp "$f" "$OUTPUT_DIR/rv64um/" 2>/dev/null || true
done
for f in "$TESTS_DIR/isa/rv64ua-p-"*; do
    [ -f "$f" ] && [ ! "${f##*.}" = "dump" ] && cp "$f" "$OUTPUT_DIR/rv64ua/" 2>/dev/null || true
done

# Count binaries
RV32_COUNT=$(find "$OUTPUT_DIR/rv32ui" "$OUTPUT_DIR/rv32um" "$OUTPUT_DIR/rv32ua" -type f 2>/dev/null | wc -l)
RV64_COUNT=$(find "$OUTPUT_DIR/rv64ui" "$OUTPUT_DIR/rv64um" "$OUTPUT_DIR/rv64ua" -type f 2>/dev/null | wc -l)

echo "Done! Copied $RV32_COUNT RV32 tests and $RV64_COUNT RV64 tests to $OUTPUT_DIR"
