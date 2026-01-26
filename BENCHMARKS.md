# Benchmark Results

## System Information

| Property | Value |
|----------|-------|
| Kernel | Linux 6.17.12-400.asahi.fc43.aarch64+16k |
| Architecture | aarch64 |
| OS | Fedora Linux Asahi Remix 43 (Workstation Edition) |
| Rust | rustc 1.94.0-nightly (22c74ba91 2026-01-15) |
| Clang | clang version 21.1.8 (Fedora 21.1.8-1.fc43) |
| Date | 2026-01-26 22:59:54 |

## towers

*Towers of Hanoi (recursive) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   403.00ns |   1.0x |            - |     - |           - |
| rvr-rv32i      |      9.56K |     29.05K |      3.0x |     9.75us |  24.2x |    1.21 BIPS |  4.84 |       0.30% |
| rvr-rv64i      |      8.44K |     30.77K |      3.6x |    10.17us |  25.2x |     981 MIPS |  3.51 |       0.26% |
| rvr-rv32e      |      9.56K |     29.05K |      3.0x |    10.44us |  25.9x |    1.27 BIPS |  5.57 |       0.19% |
| rvr-rv64e      |      8.44K |     30.77K |      3.6x |    10.67us |  26.5x |     883 MIPS |  3.05 |       0.34% |

## qsort

*Quick sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    62.86us |   1.0x |            - |     - |           - |
| rvr-rv32e      |    228.99K |    455.73K |      2.0x |    79.67us |   1.3x |    2.94 BIPS |  2.48 |       6.47% |
| rvr-rv32i      |    228.99K |    455.73K |      2.0x |    81.15us |   1.3x |    2.89 BIPS |  2.53 |       6.49% |
| rvr-rv64i      |    228.90K |    471.05K |      2.1x |    81.93us |   1.3x |    2.85 BIPS |  2.41 |       7.61% |
| rvr-rv64e      |    228.90K |    471.05K |      2.1x |    84.08us |   1.3x |    2.75 BIPS |  2.28 |       7.71% |

## rsort

*Radix sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |     6.65us |   1.0x |            - |     - |           - |
| rvr-rv32e      |    367.88K |    558.80K |      1.5x |    47.54us |   7.1x |    7.76 BIPS |  4.49 |       0.06% |
| rvr-rv32i      |    367.88K |    558.80K |      1.5x |    48.24us |   7.3x |    7.66 BIPS |  4.63 |       0.06% |
| rvr-rv64e      |    368.39K |    643.04K |      1.7x |    50.75us |   7.6x |    7.29 BIPS |  4.87 |       0.06% |
| rvr-rv64i      |    368.39K |    643.04K |      1.7x |    52.85us |   7.9x |    6.98 BIPS |  4.59 |       0.05% |

## median

*Median filter | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   833.33ns |   1.0x |            - |     - |           - |
| rvr-rv32i      |     12.97K |     30.42K |      2.3x |    10.01us |  12.0x |    1.52 BIPS |  3.63 |       0.30% |
| rvr-rv64e      |     12.86K |     30.51K |      2.4x |    10.32us |  12.4x |    1.41 BIPS |  3.49 |       0.30% |
| rvr-rv64i      |     12.86K |     30.51K |      2.4x |    10.81us |  13.0x |    1.28 BIPS |  3.27 |       0.22% |
| rvr-rv32e      |     12.97K |     30.42K |      2.3x |    14.22us |  17.1x |     991 MIPS |  1.27 |       0.17% |

## multiply

*Software multiply | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   402.67ns |   1.0x |            - |     - |           - |
| rvr-rv32e      |     56.92K |     33.73K |      0.6x |    10.89us |  27.0x |    5.75 BIPS |  2.34 |       0.86% |
| rvr-rv32i      |     56.92K |     33.73K |      0.6x |    11.35us |  28.2x |    5.26 BIPS |  2.83 |       0.94% |
| rvr-rv64e      |     55.62K |     72.24K |      1.3x |    12.32us |  30.6x |    4.78 BIPS |  4.45 |       0.17% |
| rvr-rv64i      |     55.62K |     72.24K |      1.3x |    12.44us |  30.9x |    4.71 BIPS |  4.36 |       0.57% |

## vvadd

*Vector-vector addition | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    97.33ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |      7.84K |     15.48K |      2.0x |     7.03us |  72.2x |    1.23 BIPS |  3.43 |       0.26% |
| rvr-rv64e      |      7.84K |     15.48K |      2.0x |     7.44us |  76.5x |    1.13 BIPS |  2.79 |       0.36% |
| rvr-rv32e      |      8.81K |     17.57K |      2.0x |     8.18us |  84.0x |    1.23 BIPS |  2.92 |       0.56% |
| rvr-rv32i      |      8.81K |     17.57K |      2.0x |     8.74us |  89.8x |    1.18 BIPS |  3.68 |       0.53% |

## memcpy

*Memory copy operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   708.67ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |     28.88K |     52.97K |      1.8x |    10.35us |  14.6x |    3.04 BIPS |  5.17 |       0.26% |
| rvr-rv64e      |     28.88K |     52.97K |      1.8x |    10.76us |  15.2x |    2.84 BIPS |  4.76 |       0.28% |
| rvr-rv32e      |     33.81K |     62.10K |      1.8x |    11.17us |  15.8x |    3.24 BIPS |  4.06 |       0.09% |
| rvr-rv32i      |     33.81K |     62.10K |      1.8x |    11.44us |  16.1x |    3.26 BIPS |  5.33 |       0.11% |

## dhrystone

*Classic Dhrystone benchmark | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    10.49us |   1.0x |            - |     - |           - |
| rvr-rv32i      |    210.71K |    461.95K |      2.2x |    29.90us |   2.9x |    7.34 BIPS |  7.09 |       0.02% |
| rvr-rv32e      |    210.71K |    461.95K |      2.2x |    31.54us |   3.0x |    6.92 BIPS |  6.77 |       0.02% |
| rvr-rv64e      |    198.11K |    497.05K |      2.5x |    31.86us |   3.0x |    6.36 BIPS |  6.84 |       0.03% |
| rvr-rv64i      |    198.11K |    497.05K |      2.5x |    32.44us |   3.1x |    6.21 BIPS |  6.37 |       0.03% |

## fib

*Fibonacci (recursive tail-call) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    85.82ms |   1.0x |   14.92 BIPS |  5.00 |       0.00% |
| host           |          - |      1.28B |         - |    86.18ms |   1.0x |            - |  5.00 |       0.00% |
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    86.31ms |   1.0x |   14.83 BIPS |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    87.18ms |   1.0x |            - |  4.98 |       0.01% |
| libriscv-rv64i |          - |      1.28B |         - |    88.00ms |   1.0x |            - |  4.98 |       0.01% |
| rvr-rv32i      |      1.28B |      1.28B |      1.0x |   134.03ms |   1.6x |    9.55 BIPS |  3.21 |       0.00% |
| rvr-rv32e      |      1.28B |      1.28B |      1.0x |   134.23ms |   1.6x |    9.54 BIPS |  3.21 |       0.00% |
| libriscv-rv32i |          - |      1.28B |         - |   174.92ms |   2.0x |            - |  2.46 |       0.01% |
| libriscv-rv32e |          - |      1.28B |         - |   175.23ms |   2.0x |            - |  2.46 |       0.01% |

## fib-asm

*Fibonacci (hand-written assembly) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    85.77ms |      - |   14.92 BIPS |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    86.01ms |      - |   14.88 BIPS |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    86.54ms |      - |            - |  4.98 |       0.01% |
| libriscv-rv64i |          - |      1.28B |         - |    87.15ms |      - |            - |  4.98 |       0.01% |

## coremark

*CoreMark CPU benchmark (EEMBC) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    111.78B |         - |     12.84s |   1.0x |            - |  2.91 |       0.18% |
| rvr-rv32e      |    109.57B |    157.71B |      1.4x |     16.02s |   1.2x |    6.84 BIPS |  3.26 |       0.17% |
| rvr-rv32i      |    109.57B |    157.71B |      1.4x |     16.03s |   1.2x |    6.84 BIPS |  3.26 |       0.17% |
| rvr-rv64i      |    134.05B |    175.57B |      1.3x |     18.01s |   1.4x |    7.44 BIPS |  3.23 |       0.16% |
| rvr-rv64e      |    134.05B |    175.57B |      1.3x |     18.02s |   1.4x |    7.44 BIPS |  3.23 |       0.16% |
| libriscv-rv32i |          - |    422.26B |         - |     29.17s |   2.3x |            - |  4.79 |       0.09% |
| libriscv-rv32e |          - |    422.26B |         - |     29.24s |   2.3x |            - |  4.78 |       0.09% |
| libriscv-rv64e |          - |    585.58B |         - |     38.16s |   3.0x |            - |  5.08 |       0.11% |
| libriscv-rv64i |          - |    585.58B |         - |     38.17s |   3.0x |            - |  5.08 |       0.11% |

## minimal

*Minimal function call overhead | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |        184 |         - |    28.00ns |   1.0x |            - |  0.68 |       7.32% |
| rvr-rv32e      |         30 |        429 |     14.3x |   250.00ns |   8.9x |     120 MIPS |  0.43 |       4.00% |
| rvr-rv64e      |         30 |        430 |     14.3x |   250.33ns |   8.9x |     120 MIPS |  0.41 |       4.00% |
| rvr-rv32i      |         30 |        438 |     14.6x |   264.00ns |   9.4x |     114 MIPS |  0.38 |       4.00% |
| rvr-rv64i      |         30 |        440 |     14.7x |   264.00ns |   9.4x |     114 MIPS |  0.38 |       4.00% |

## prime-sieve

*Prime number sieve algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     10.74M |         - |     1.83ms |   1.0x |            - |  1.95 |       0.26% |
| rvr-rv64e      |     17.59M |     20.00M |      1.1x |     2.02ms |   1.1x |    8.69 BIPS |  3.27 |       0.20% |
| rvr-rv64i      |     16.18M |     19.99M |      1.2x |     2.13ms |   1.2x |    7.61 BIPS |  3.10 |       0.20% |
| rvr-rv32e      |     23.79M |     26.26M |      1.1x |     2.35ms |   1.3x |   10.14 BIPS |  3.69 |       0.18% |
| rvr-rv32i      |     21.36M |     28.87M |      1.4x |     2.61ms |   1.4x |    8.19 BIPS |  3.66 |       0.18% |

## pinky

*NES emulator (cycle-accurate) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     28.53M |         - |     2.53ms |   1.0x |            - |  3.73 |       1.48% |
| rvr-rv64e      |     30.81M |     44.00M |      1.4x |     3.31ms |   1.3x |    9.31 BIPS |  4.41 |       1.73% |
| rvr-rv32e      |     31.90M |     44.69M |      1.4x |     3.40ms |   1.3x |    9.39 BIPS |  4.35 |       1.69% |
| rvr-rv32i      |     31.12M |     45.21M |      1.5x |     3.44ms |   1.4x |    9.04 BIPS |  4.35 |       1.66% |
| rvr-rv64i      |     30.88M |     46.11M |      1.5x |     3.53ms |   1.4x |    8.74 BIPS |  4.32 |       1.74% |

## memset

*Memory set operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    327.87K |         - |    87.13us |   1.0x |            - |  1.24 |       0.01% |
| rvr-rv64i      |      1.57M |      3.15M |      2.0x |   224.75us |   2.6x |    7.00 BIPS |  4.61 |       0.00% |
| rvr-rv64e      |      1.57M |      3.15M |      2.0x |   227.46us |   2.6x |    6.92 BIPS |  4.61 |       0.00% |
| rvr-rv32e      |      3.15M |      6.29M |      2.0x |   452.06us |   5.2x |    6.96 BIPS |  4.61 |       0.00% |
| rvr-rv32i      |      3.15M |      6.29M |      2.0x |   453.82us |   5.2x |    6.93 BIPS |  4.61 |       0.00% |

## reth

*Reth block validator | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      3.36B |      3.49B |      1.0x |   233.99ms |      - |   14.36 BIPS |  4.90 |       1.22% |
| rvr-rv64i      |      2.88B |      3.39B |      1.2x |   241.84ms |      - |   11.93 BIPS |  4.66 |       1.20% |
| rvr-rv32i      |      7.27B |      9.36B |      1.3x |   620.28ms |      - |   11.72 BIPS |  4.99 |       0.77% |
| rvr-rv32e      |      8.55B |      9.81B |      1.1x |   640.31ms |      - |   13.35 BIPS |  5.10 |       0.81% |
| host           |          - |          - |         - |          - |      - | build failed |     - |           - |

