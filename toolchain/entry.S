# Minimal entry point for rvr benchmarks
# This _start is never called directly - we use symbol lookup to call initialize/run
# But the linker requires an entry point

.section .text._start
.global _start
.type _start, @function
_start:
    # Set up global pointer (required for some code models)
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop

    # Set up stack pointer
    la sp, __stack_top

    # Trap - this should never be reached
    # We call initialize/run directly via symbol lookup
    ebreak

.size _start, . - _start
