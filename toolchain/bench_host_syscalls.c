// Host-compatible syscalls for riscv-tests benchmarks.
// Provides setStats() using clock_gettime instead of CSRs.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static uint64_t start_nanos;
static uint64_t elapsed_nanos;
static int stats_printed = 0;

static uint64_t get_nanos(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

static void print_stats(void) {
    if (!stats_printed && elapsed_nanos > 0) {
        printf("host_nanos = %lu\n", (unsigned long)elapsed_nanos);
        stats_printed = 1;
    }
}

void setStats(int enable) {
    if (enable) {
        start_nanos = get_nanos();
        atexit(print_stats);
    } else {
        elapsed_nanos = get_nanos() - start_nanos;
    }
}
