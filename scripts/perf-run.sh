#!/bin/bash
# Run a command with perf stat, measuring instructions, cycles, and branch stats
# Adapted from Mojo's perf-run.sh

if command -v perf &> /dev/null && perf stat -e instructions true 2>/dev/null; then
    # Use perf stat for time, instruction count, cycles, and branch stats
    output=$(perf stat -e instructions,cycles,branches,branch-misses "$@" 2>&1)
    exit_code=$?

    # Extract instructions (handles both standard Linux and Apple Silicon formats)
    instrs=$(echo "$output" | grep -i "instructions" | head -1 | awk '{print $1}' | tr -d ',')

    # Extract cycles
    cycles=$(echo "$output" | grep -i "cycles" | head -1 | awk '{print $1}' | tr -d ',')

    # Extract elapsed time (format: "0.123456789 seconds time elapsed")
    elapsed=$(echo "$output" | grep "seconds time elapsed" | awk '{print $1}')

    # Extract branch stats
    branches=$(echo "$output" | grep -i "branches" | grep -v "branch-misses" | head -1 | awk '{print $1}' | tr -d ',')
    branch_misses=$(echo "$output" | grep -i "branch-misses" | head -1 | awk '{print $1}' | tr -d ',')

    # Format instruction count with suffix (K, M, B)
    if [[ -n "$instrs" && "$instrs" =~ ^[0-9]+$ ]]; then
        if (( instrs >= 1000000000 )); then
            instrs_fmt=$(printf "%.2fB" $(echo "$instrs / 1000000000" | bc -l))
        elif (( instrs >= 1000000 )); then
            instrs_fmt=$(printf "%.2fM" $(echo "$instrs / 1000000" | bc -l))
        elif (( instrs >= 1000 )); then
            instrs_fmt=$(printf "%.2fK" $(echo "$instrs / 1000" | bc -l))
        else
            instrs_fmt="$instrs"
        fi
        echo "instret:    $instrs_fmt"
    fi

    # Print cycles and IPC if available
    if [[ -n "$cycles" && "$cycles" =~ ^[0-9]+$ && -n "$instrs" && "$instrs" =~ ^[0-9]+$ && "$cycles" -gt 0 ]]; then
        ipc=$(echo "scale=2; $instrs / $cycles" | bc -l)
        if (( cycles >= 1000000000 )); then
            cycles_fmt=$(printf "%.2fB" $(echo "$cycles / 1000000000" | bc -l))
        elif (( cycles >= 1000000 )); then
            cycles_fmt=$(printf "%.2fM" $(echo "$cycles / 1000000" | bc -l))
        elif (( cycles >= 1000 )); then
            cycles_fmt=$(printf "%.2fK" $(echo "$cycles / 1000" | bc -l))
        else
            cycles_fmt="$cycles"
        fi
        echo "cycles:     $cycles_fmt ($ipc IPC)"
    fi

    echo "time:       ${elapsed}s"

    # Print branch stats if available
    if [[ -n "$branches" && "$branches" =~ ^[0-9]+$ && -n "$branch_misses" && "$branch_misses" =~ ^[0-9]+$ ]]; then
        if (( branches > 0 )); then
            miss_rate=$(echo "scale=2; $branch_misses * 100 / $branches" | bc -l)
            # Format branch count
            if (( branches >= 1000000000 )); then
                branches_fmt=$(printf "%.2fB" $(echo "$branches / 1000000000" | bc -l))
            elif (( branches >= 1000000 )); then
                branches_fmt=$(printf "%.2fM" $(echo "$branches / 1000000" | bc -l))
            elif (( branches >= 1000 )); then
                branches_fmt=$(printf "%.2fK" $(echo "$branches / 1000" | bc -l))
            else
                branches_fmt="$branches"
            fi
            # Format miss count
            if (( branch_misses >= 1000000000 )); then
                misses_fmt=$(printf "%.2fB" $(echo "$branch_misses / 1000000000" | bc -l))
            elif (( branch_misses >= 1000000 )); then
                misses_fmt=$(printf "%.2fM" $(echo "$branch_misses / 1000000" | bc -l))
            elif (( branch_misses >= 1000 )); then
                misses_fmt=$(printf "%.2fK" $(echo "$branch_misses / 1000" | bc -l))
            else
                misses_fmt="$branch_misses"
            fi
            echo "branches:   $branches_fmt, $misses_fmt misses (${miss_rate}%)"
        fi
    fi

    exit $exit_code
else
    # Fallback to bash time
    TIMEFORMAT="time:       %Rs"
    time "$@"
fi
