#!/bin/bash
#
# Run riscv-tests and report results
#
# Usage: ./scripts/run-riscv-tests.sh [--verbose]
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# Tests to skip (not compatible with static recompilation)
SKIP_TESTS=(
    # fence.i tests self-modifying code - incompatible with static recompilation
    "rv32ui-p-fence_i"
    "rv64ui-p-fence_i"
    # Machine/supervisor mode tests require privilege features
    # (filtered by pattern below, but listed here for documentation)
)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

VERBOSE=0
if [[ "$1" == "--verbose" || "$1" == "-v" ]]; then
    VERBOSE=1
fi

is_skipped() {
    local name="$1"
    for skip in "${SKIP_TESTS[@]}"; do
        if [[ "$name" == "$skip" ]]; then
            return 0
        fi
    done
    return 1
}

PASSED=0
FAILED=0
SKIPPED=0
FAILURES=""
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# Find all user-level tests (exclude mi/si privilege tests)
tests=$(find bin/riscv/tests -type f -name "rv*-p-*" | grep -v "mi-p-" | grep -v "si-p-" | sort)
total=$(echo "$tests" | wc -l)
current=0

for test in $tests; do
    name=$(basename "$test")
    current=$((current + 1))

    # Check skip list
    if is_skipped "$name"; then
        SKIPPED=$((SKIPPED + 1))
        if [[ $VERBOSE -eq 1 ]]; then
            echo -e "[$current/$total] ${YELLOW}SKIP${NC} $name"
        fi
        continue
    fi

    outdir="$TMPDIR/out"
    rm -rf "$outdir"

    # Compile
    if ! cargo run --release -q -- compile --tohost -o "$outdir" "$test" >/dev/null 2>&1; then
        FAILED=$((FAILED + 1))
        FAILURES="$FAILURES  $name (compile failed)\n"
        echo -e "[$current/$total] ${RED}FAIL${NC} $name (compile failed)"
        continue
    fi

    # Run with timeout
    result=$(timeout 5s cargo run --release -q -- run "$outdir" 2>&1) || true
    exit_code=$(echo "$result" | grep "Exit code:" | awk '{print $3}')

    if [[ -z "$exit_code" ]]; then
        FAILED=$((FAILED + 1))
        FAILURES="$FAILURES  $name (crash/timeout)\n"
        echo -e "[$current/$total] ${RED}FAIL${NC} $name (crash/timeout)"
    elif [[ "$exit_code" == "0" ]]; then
        PASSED=$((PASSED + 1))
        if [[ $VERBOSE -eq 1 ]]; then
            echo -e "[$current/$total] ${GREEN}PASS${NC} $name"
        fi
    else
        FAILED=$((FAILED + 1))
        FAILURES="$FAILURES  $name (exit=$exit_code)\n"
        echo -e "[$current/$total] ${RED}FAIL${NC} $name (exit=$exit_code)"
    fi
done

echo ""
echo "================================"
echo -e "${GREEN}PASSED${NC}: $PASSED"
echo -e "${RED}FAILED${NC}: $FAILED"
echo -e "${YELLOW}SKIPPED${NC}: $SKIPPED"
echo ""

if [[ -n "$FAILURES" ]]; then
    echo "Failures:"
    echo -e "$FAILURES"
    exit 1
fi
