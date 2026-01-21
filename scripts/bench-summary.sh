#!/bin/bash
# Benchmark summary script - compiles and runs reth benchmarks and generates markdown table
#
# Usage: ./scripts/bench-summary.sh [options]
#
# Options:
#   -r, --runs N       Number of runs for averaging (default: 3)
#   -a, --arch ARCHS   Comma-separated architectures: rv32i,rv32e,rv64i,rv64e (default: all)
#   -t, --trace        Enable tracing
#   -f, --fast         Use fast mode (no instret counting)
#   -v, --verbose      Verbose output
#   -n, --no-compile   Skip compilation (use existing .so)
#   -h, --help         Show this help
#
# Examples:
#   ./scripts/bench-summary.sh                      # All archs, no trace
#   ./scripts/bench-summary.sh -t                   # All archs with trace
#   ./scripts/bench-summary.sh -f                   # All archs, fast mode (no instret)
#   ./scripts/bench-summary.sh -a rv64i             # Only RV64I
#   ./scripts/bench-summary.sh -a rv64i,rv64e -t    # RV64I/E with trace
#   ./scripts/bench-summary.sh -r 5 -a rv32i -v     # 5 runs, RV32I only, verbose

set -e

# Defaults
RUNS=3
VERBOSE=""
TRACE=false
FAST=false
NO_COMPILE=false
ARCHS="rv32i,rv32e,rv64i,rv64e"

show_help() {
    sed -n '2,/^$/p' "$0" | sed 's/^# //' | sed 's/^#//'
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -r|--runs)       RUNS="$2"; shift 2 ;;
        -a|--arch)       ARCHS="$2"; shift 2 ;;
        -t|--trace)      TRACE=true; shift ;;
        -f|--fast)       FAST=true; shift ;;
        -v|--verbose)    VERBOSE="-v"; shift ;;
        -n|--no-compile) NO_COMPILE=true; shift ;;
        -h|--help)       show_help ;;
        *)               echo "Unknown option: $1"; show_help ;;
    esac
done

# Parse arch list into flags
RUN_RV32I=false; RUN_RV32E=false; RUN_RV64I=false; RUN_RV64E=false
IFS=',' read -ra ARCH_LIST <<< "$ARCHS"
for arch in "${ARCH_LIST[@]}"; do
    case "$arch" in
        rv32i) RUN_RV32I=true ;;
        rv32e) RUN_RV32E=true ;;
        rv64i) RUN_RV64I=true ;;
        rv64e) RUN_RV64E=true ;;
        *) echo "Unknown arch: $arch (valid: rv32i,rv32e,rv64i,rv64e)"; exit 1 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

log() { [[ "$VERBOSE" == "-v" ]] && echo "$@" >&2 || true; }
die() { echo "ERROR: $1" >&2; exit 1; }

# Build rvr if needed
if [[ ! -f "$PROJECT_DIR/target/release/rvr" ]]; then
    log "Building rvr (release)..."
    cargo build --release --package rvr
fi
RVR="$PROJECT_DIR/target/release/rvr"

# Determine compile options based on flags
get_compile_opts() {
    local opts=""
    if $TRACE; then
        opts="$opts --tracer stats"
    fi
    if $FAST; then
        opts="$opts --instret off"
    fi
    echo "$opts"
}

# Compile a RISC-V ELF to native
compile_variant() {
    local elf=$1
    local out_dir=$2
    local opts=$(get_compile_opts)

    if $NO_COMPILE && [[ -d "$out_dir" ]]; then
        log "Skipping compile (--no-compile): $out_dir"
        return 0
    fi

    log "Compiling $elf -> $out_dir"
    if [[ "$VERBOSE" == "-v" ]]; then
        $RVR compile "$elf" -o "$out_dir" $opts
    else
        $RVR compile "$elf" -o "$out_dir" $opts >/dev/null 2>&1
    fi
}

# Run a compiled program and extract metrics
run_variant() {
    local lib_dir=$1
    local output
    output=$($RVR run "$lib_dir" --format mojo --runs "$RUNS" 2>&1)

    local instret=$(echo "$output" | grep "^instret:" | awk '{print $2}')
    local time=$(echo "$output" | grep "^time:" | awk '{print $2}')
    local mips=$(echo "$output" | grep "^speed:" | awk '{print $2}')

    [[ -z "$instret" ]] && instret="-"
    [[ -z "$time" ]] && time="-"
    [[ -z "$mips" ]] && mips="-"

    echo "$instret $time $mips"
}

# Run with perf stats
run_variant_perf() {
    local lib_dir=$1
    local output
    output=$("$SCRIPT_DIR/perf-run.sh" $RVR run "$lib_dir" --format mojo --runs "$RUNS" 2>&1)

    local instret=$(echo "$output" | grep "^instret:" | awk '{print $2}')
    local time=$(echo "$output" | grep "^time:" | awk '{print $2}' | sed 's/s$//')
    local mips=$(echo "$output" | grep "^speed:" | awk '{print $2}')
    local cycles_line=$(echo "$output" | grep "^cycles:")
    local ipc="-"
    if [[ -n "$cycles_line" ]]; then
        ipc=$(echo "$cycles_line" | sed -n 's/.*(\([0-9.]*\) IPC).*/\1/p')
    fi
    local branch_line=$(echo "$output" | grep "^branches:")
    local branch_miss_rate="-"
    if [[ -n "$branch_line" ]]; then
        branch_miss_rate=$(echo "$branch_line" | sed -n 's/.*(\([0-9.]*\)%).*/\1/p')
    fi

    [[ -z "$instret" ]] && instret="-"
    [[ -z "$time" ]] && time="-"
    [[ -z "$mips" ]] && mips="-"

    echo "$instret $time $mips $ipc $branch_miss_rate"
}

# Format number with suffix (K, M, B)
fmt_num() {
    local n=$1
    if [[ ! "$n" =~ ^[0-9]+$ ]]; then
        echo "$n"
        return
    fi
    if (( n >= 1000000000 )); then
        printf "%.2fB" $(echo "$n / 1000000000" | bc -l)
    elif (( n >= 1000000 )); then
        printf "%.2fM" $(echo "$n / 1000000" | bc -l)
    elif (( n >= 1000 )); then
        printf "%.2fK" $(echo "$n / 1000" | bc -l)
    else
        echo "$n"
    fi
}

# Check if reth-validator ELF exists for an arch
check_elf() {
    local arch=$1
    local elf="$PROJECT_DIR/bin/reth/$arch/reth-validator"
    if [[ ! -f "$elf" ]]; then
        echo "Warning: $elf not found. Run ./scripts/reth-build.sh $arch first." >&2
        return 1
    fi
    return 0
}

# Run benchmark for one arch
bench_arch() {
    local arch=$1
    local elf="$PROJECT_DIR/bin/reth/$arch/reth-validator"

    if ! check_elf "$arch"; then
        echo "| $arch | - | - | - | - | - |"
        return
    fi

    local suffix=""
    $TRACE && suffix="-trace"
    $FAST && suffix="${suffix}-fast"
    [[ -n "$suffix" ]] && suffix="${suffix#-}"  # Remove leading dash if present
    [[ -z "$suffix" ]] && suffix="base"

    local out_dir="$PROJECT_DIR/target/$arch/reth-$suffix"

    compile_variant "$elf" "$out_dir"

    if [[ ! -d "$out_dir" ]]; then
        echo "| $arch | - | - | - | - | - |"
        return
    fi

    log "Running $arch ($suffix)..."
    local result=$(run_variant_perf "$out_dir")
    read -r instret time mips ipc branch_miss <<< "$result"

    # Format for table
    local instret_fmt=$(fmt_num "$instret")

    echo "| $arch | $instret_fmt | ${time}s | $mips MIPS | $ipc | ${branch_miss}% |"
}

# Print markdown table header
print_header() {
    echo ""
    echo "## Benchmark Results"
    echo ""
    local mode="Base"
    $TRACE && mode="Trace"
    $FAST && mode="Fast (no instret)"
    echo "Mode: **$mode** | Runs: **$RUNS**"
    echo ""
    echo "| Arch | Instret | Time | Speed | IPC | Branch Miss |"
    echo "|------|---------|------|-------|-----|-------------|"
}

# Main
main() {
    print_header

    $RUN_RV32I && bench_arch rv32i
    $RUN_RV32E && bench_arch rv32e
    $RUN_RV64I && bench_arch rv64i
    $RUN_RV64E && bench_arch rv64e

    echo ""
}

main
