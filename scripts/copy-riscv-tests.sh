#!/usr/bin/env bash
# Copy riscv-tests binaries from openvm-mojo to rvr
#
# The openvm-mojo project already has pre-built riscv-tests binaries.
# This script copies them to the rvr project.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$ROOT_DIR/bin/riscv/tests"

# Source directory (adjust if needed)
SOURCE_DIR="${OPENVM_MOJO_DIR:-/home/ayush/projects/openvm-mojo}/bin/riscv/tests"

if [ ! -d "$SOURCE_DIR" ]; then
    echo "Error: Source directory not found: $SOURCE_DIR"
    echo "Set OPENVM_MOJO_DIR environment variable or run build-riscv-tests.sh"
    exit 1
fi

# Create output directories
mkdir -p "$OUTPUT_DIR"

# Copy test directories
for dir in rv32ui rv32um rv32ua rv64ui rv64um rv64ua; do
    if [ -d "$SOURCE_DIR/$dir" ]; then
        echo "Copying $dir..."
        cp -r "$SOURCE_DIR/$dir" "$OUTPUT_DIR/"
    fi
done

# Count binaries
TOTAL=$(find "$OUTPUT_DIR" -type f 2>/dev/null | wc -l)
echo "Done! Copied $TOTAL test binaries to $OUTPUT_DIR"
