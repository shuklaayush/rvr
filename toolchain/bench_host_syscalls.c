// Host-compatible syscalls for riscv-tests benchmarks.
// Provides setStats() using clock_gettime instead of CSRs.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static uint64_t start_nanos;
static uint64_t elapsed_nanos;

static uint64_t get_nanos(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

void setStats(int enable) {
    if (enable) {
        start_nanos = get_nanos();
    } else {
        elapsed_nanos = get_nanos() - start_nanos;
        // Print in parseable format - this is what we measure
        printf("host_nanos = %lu\n", (unsigned long)elapsed_nanos);
    }
}
