#!/usr/bin/env bash
# Build polkavm benchmarks for rvr
#
# Builds the polkavm benchmarks for multiple RISC-V architectures.
# Each benchmark exports initialize() and run() for direct calling via rvr.
#
# Usage: ./build.sh [options] [benchmark-name]
#   Without benchmark name: builds all benchmarks
#   With benchmark name: builds only the specified benchmark
#
# Options:
#   --arch <arch>   Build only for specified arch (rv32i,rv32e,rv64i,rv64e,all)
#   --host          Also build native host binary for comparison

set -euo pipefail

SCRIPT_DIR="$(cd "${0%/*}" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_DIR="$SCRIPT_DIR/targets"

# Default values
BUILD_HOST=false
ARCHS="rv64i"
BENCHMARK=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --arch)
            ARCHS="$2"
            shift 2
            ;;
        --host)
            BUILD_HOST=true
            shift
            ;;
        -*)
            echo "Unknown option: $1"
            exit 1
            ;;
        *)
            BENCHMARK="$1"
            shift
            ;;
    esac
done

# Expand "all" to all architectures
if [ "$ARCHS" = "all" ]; then
    ARCHS="rv32i,rv32e,rv64i,rv64e"
fi

cd "$SCRIPT_DIR/polkavm/guest-programs"

# Check for required tools
if ! command -v clang &> /dev/null; then
    echo "Error: clang is required to compile entry.S"
    echo "Install with: sudo dnf install clang  (or apt install clang)"
    exit 1
fi

# Compile entry.S for each architecture (32-bit and 64-bit, I and E variants)
compile_entry() {
    local arch=$1
    local obj_file="$TARGET_DIR/entry_${arch}.o"

    case "$arch" in
        rv32i) local target="riscv32"; local march="rv32imac" ;;
        rv32e) local target="riscv32"; local march="rv32emac" ;;
        rv64i) local target="riscv64"; local march="rv64imac" ;;
        rv64e) local target="riscv64"; local march="rv64emac" ;;
        *) echo "Unknown arch: $arch"; exit 1 ;;
    esac

    if [ ! -f "$obj_file" ] || [ "$TARGET_DIR/entry.S" -nt "$obj_file" ]; then
        echo "Compiling entry.S for $arch..."
        clang --target=$target -march=$march -c "$TARGET_DIR/entry.S" -o "$obj_file"
    fi
}

# All benchmarks
ALL_BENCHMARKS=("bench-minimal" "bench-pinky" "bench-prime-sieve" "bench-memset")

# Output name mapping (bench-X -> X)
get_output_name() {
    echo "${1#bench-}"
}

# If benchmark specified, use only that one
if [ -n "$BENCHMARK" ]; then
    # Allow shorthand names (minimal -> bench-minimal)
    if [[ "$BENCHMARK" != bench-* ]]; then
        BENCHMARK="bench-$BENCHMARK"
    fi
    BENCHMARKS=("$BENCHMARK")
else
    BENCHMARKS=("${ALL_BENCHMARKS[@]}")
fi

# Build for each architecture
IFS=',' read -ra ARCH_ARRAY <<< "$ARCHS"
for arch in "${ARCH_ARRAY[@]}"; do
    arch=$(echo "$arch" | tr -d ' ')  # trim whitespace

    echo ""
    echo "=== Building for $arch ==="

    # Compile entry point
    compile_entry "$arch"
    ENTRY_OBJ="$TARGET_DIR/entry_${arch}.o"

    # Determine target features based on arch
    if [[ "$arch" == rv32* ]]; then
        TARGET_FEATURES="+zba,+zbb,+zbs"
    else
        TARGET_FEATURES="+zba,+zbb,+zbs"
    fi

    for bench in "${BENCHMARKS[@]}"; do
        output_name=$(get_output_name "$bench")
        out_dir="$PROJECT_ROOT/bin/$arch"
        mkdir -p "$out_dir"

        echo "Building $bench -> $out_dir/$output_name"

        RUSTFLAGS="-C target-feature=$TARGET_FEATURES -C link-arg=$ENTRY_OBJ -C link-arg=-T$TARGET_DIR/link.x -C link-arg=--undefined=initialize -C link-arg=--undefined=run" \
            cargo build \
            -Z build-std=core,alloc \
            -Z build-std-features=compiler-builtins-mem \
            --target="$TARGET_DIR/${arch}.json" \
            --release \
            --bin "$bench" \
            -p "$bench" \
            2>&1 | grep -v "^\s*Compiling\|^\s*Finished\|^\s*warning:" || true

        cp "target/$arch/release/$bench" "$out_dir/$output_name"
    done
done

# Build host binaries if requested
if [ "$BUILD_HOST" = true ]; then
    echo ""
    echo "=== Building host binaries ==="
    echo "Note: Host builds require a wrapper with main(). Creating stubs..."

    # For now, just print a message - full host support needs wrapper code
    echo "Host binary support not yet implemented for polkavm benchmarks."
    echo "The benchmarks use #![no_main] and need a wrapper to run natively."
fi

echo ""
echo "Build complete."
