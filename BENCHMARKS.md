# Benchmark Results

## System Information

| Property | Value |
|----------|-------|
| Kernel | Linux 6.17.12-400.asahi.fc43.aarch64+16k |
| Architecture | aarch64 |
| OS | Fedora Linux Asahi Remix 43 (Workstation Edition) |
| Rust | rustc 1.94.0-nightly (22c74ba91 2026-01-15) |
| Clang | clang version 21.1.8 (Fedora 21.1.8-1.fc43) |
| Date | 2026-01-26 23:15:40 |

## towers

*Towers of Hanoi (recursive) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   389.00ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |      8.44K |     30.77K |      3.6x |     9.72us |  25.0x |    1.04 BIPS |  3.59 |       0.42% |
| rvr-rv64i      |      8.44K |     30.77K |      3.6x |    10.14us |  26.1x |     921 MIPS |  3.11 |       0.18% |
| rvr-rv32e      |      9.56K |     29.05K |      3.0x |    12.35us |  31.7x |     872 MIPS |  1.57 |       0.30% |
| rvr-rv32i      |      9.56K |     29.05K |      3.0x |    13.83us |  35.6x |     920 MIPS |  2.46 |       0.23% |

## qsort

*Quick sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    68.40us |   1.0x |            - |     - |           - |
| rvr-rv32i      |    228.99K |    455.73K |      2.0x |    77.93us |   1.1x |    3.04 BIPS |  2.60 |       6.30% |
| rvr-rv64e      |    228.90K |    471.05K |      2.1x |    81.36us |   1.2x |    2.87 BIPS |  2.42 |       7.59% |
| rvr-rv32e      |    228.99K |    455.73K |      2.0x |    81.75us |   1.2x |    2.87 BIPS |  2.57 |       6.24% |
| rvr-rv64i      |    228.90K |    471.05K |      2.1x |    83.96us |   1.2x |    2.79 BIPS |  2.39 |       7.54% |

## rsort

*Radix sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |     6.65us |   1.0x |            - |     - |           - |
| rvr-rv32i      |    367.88K |    558.80K |      1.5x |    47.15us |   7.1x |    7.87 BIPS |  4.66 |       0.06% |
| rvr-rv32e      |    367.88K |    558.80K |      1.5x |    47.92us |   7.2x |    7.85 BIPS |  4.68 |       0.06% |
| rvr-rv64i      |    368.39K |    643.04K |      1.7x |    50.24us |   7.6x |    7.37 BIPS |  4.85 |       0.07% |
| rvr-rv64e      |    368.39K |    643.04K |      1.7x |    50.81us |   7.6x |    7.28 BIPS |  4.83 |       0.06% |

## median

*Median filter | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   555.67ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |     12.86K |     30.51K |      2.4x |     9.64us |  17.3x |    1.39 BIPS |  2.42 |       0.08% |
| rvr-rv64i      |     12.86K |     30.51K |      2.4x |    10.06us |  18.1x |    1.49 BIPS |  3.75 |       0.21% |
| rvr-rv32e      |     12.97K |     30.42K |      2.3x |    10.19us |  18.3x |    1.58 BIPS |  3.69 |       0.39% |
| rvr-rv32i      |     12.97K |     30.42K |      2.3x |    12.44us |  22.4x |    1.16 BIPS |  2.57 |       0.28% |

## multiply

*Software multiply | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   389.00ns |   1.0x |            - |     - |           - |
| rvr-rv32i      |     56.92K |     33.73K |      0.6x |     9.44us |  24.3x |    6.94 BIPS |  5.82 |       0.64% |
| rvr-rv32e      |     56.92K |     33.73K |      0.6x |    11.01us |  28.3x |    5.68 BIPS |  3.53 |       0.47% |
| rvr-rv64e      |     55.62K |     72.24K |      1.3x |    13.08us |  33.6x |    4.43 BIPS |  3.98 |       0.29% |
| rvr-rv64i      |     55.62K |     72.24K |      1.3x |    13.28us |  34.1x |    4.98 BIPS |  4.68 |       0.16% |

## vvadd

*Vector-vector addition | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    97.33ns |   1.0x |            - |     - |           - |
| rvr-rv32i      |      8.81K |     17.57K |      2.0x |     7.72us |  79.3x |    1.34 BIPS |  3.76 |       0.28% |
| rvr-rv64e      |      7.84K |     15.48K |      2.0x |     9.28us |  95.3x |     908 MIPS |  2.43 |       0.26% |
| rvr-rv32e      |      8.81K |     17.57K |      2.0x |    10.93us | 112.3x |     923 MIPS |  2.98 |       0.68% |
| rvr-rv64i      |      7.84K |     15.48K |      2.0x |    11.54us | 118.6x |     680 MIPS |  1.20 |       0.73% |

## memcpy

*Memory copy operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   833.00ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |     28.88K |     52.97K |      1.8x |     9.07us |  10.9x |    3.41 BIPS |  4.64 |       0.10% |
| rvr-rv64e      |     28.88K |     52.97K |      1.8x |    10.32us |  12.4x |    3.06 BIPS |  5.65 |       0.11% |
| rvr-rv32e      |     33.81K |     62.10K |      1.8x |    15.43us |  18.5x |    2.20 BIPS |  2.75 |       0.13% |
| rvr-rv32i      |     33.81K |     62.10K |      1.8x |    16.65us |  20.0x |    2.06 BIPS |  2.81 |       0.15% |

## dhrystone

*Classic Dhrystone benchmark | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    10.44us |   1.0x |            - |     - |           - |
| rvr-rv64i      |    198.11K |    497.05K |      2.5x |    30.88us |   3.0x |    6.51 BIPS |  6.84 |       0.02% |
| rvr-rv64e      |    198.11K |    497.05K |      2.5x |    31.46us |   3.0x |    6.40 BIPS |  7.02 |       0.03% |
| rvr-rv32e      |    210.71K |    461.95K |      2.2x |    36.13us |   3.5x |    5.83 BIPS |  5.27 |       0.02% |
| rvr-rv32i      |    210.71K |    461.95K |      2.2x |    36.92us |   3.5x |    5.73 BIPS |  5.53 |       0.03% |

## fib

*Fibonacci (recursive tail-call) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |      1.28B |         - |    84.82ms |   1.0x |            - |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.87ms |   1.0x |   15.08 BIPS |  5.00 |       0.00% |
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    85.35ms |   1.0x |   15.00 BIPS |  5.00 |       0.00% |
| libriscv-rv64i |          - |      1.28B |         - |    85.84ms |   1.0x |            - |  4.98 |       0.01% |
| libriscv-rv64e |          - |      1.28B |         - |    85.93ms |   1.0x |            - |  4.98 |       0.01% |
| rvr-rv32i      |      1.28B |      1.28B |      1.0x |   132.25ms |   1.6x |    9.68 BIPS |  3.21 |       0.00% |
| rvr-rv32e      |      1.28B |      1.28B |      1.0x |   132.31ms |   1.6x |    9.67 BIPS |  3.21 |       0.00% |
| libriscv-rv32e |          - |      1.28B |         - |   173.05ms |   2.0x |            - |  2.46 |       0.01% |
| libriscv-rv32i |          - |      1.28B |         - |   174.24ms |   2.1x |            - |  2.46 |       0.01% |

## fib-asm

*Fibonacci (hand-written assembly) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.85ms |      - |   15.09 BIPS |  5.00 |       0.00% |
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.86ms |      - |   15.08 BIPS |  5.00 |       0.00% |
| libriscv-rv64i |          - |      1.28B |         - |    86.10ms |      - |            - |  4.98 |       0.01% |
| libriscv-rv64e |          - |      1.28B |         - |    86.11ms |      - |            - |  4.98 |       0.01% |

## coremark

*CoreMark CPU benchmark (EEMBC) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    111.78B |         - |     12.83s |   1.0x |            - |  2.91 |       0.18% |
| rvr-rv32i      |    109.57B |    157.71B |      1.4x |     16.10s |   1.3x |    6.81 BIPS |  3.26 |       0.17% |
| rvr-rv32e      |    109.57B |    157.71B |      1.4x |     16.28s |   1.3x |    6.73 BIPS |  3.26 |       0.17% |
| rvr-rv64e      |    134.05B |    175.57B |      1.3x |     18.30s |   1.4x |    7.33 BIPS |  3.23 |       0.16% |
| rvr-rv64i      |    134.05B |    175.57B |      1.3x |     18.34s |   1.4x |    7.31 BIPS |  3.23 |       0.16% |
| libriscv-rv32i |          - |    422.26B |         - |     29.63s |   2.3x |            - |  4.79 |       0.09% |
| libriscv-rv32e |          - |    422.26B |         - |     29.73s |   2.3x |            - |  4.79 |       0.09% |
| libriscv-rv64i |          - |    585.58B |         - |     38.66s |   3.0x |            - |  5.08 |       0.11% |
| libriscv-rv64e |          - |    585.58B |         - |     38.77s |   3.0x |            - |  5.08 |       0.11% |

## minimal

*Minimal function call overhead | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |        184 |         - |    14.00ns |   1.0x |            - |  0.69 |       9.76% |
| rvr-rv64e      |         30 |        430 |     14.3x |   125.33ns |   9.0x |     239 MIPS |  0.55 |       5.33% |
| rvr-rv32e      |         30 |        429 |     14.3x |   166.67ns |  11.9x |     180 MIPS |  0.66 |       4.00% |
| rvr-rv32i      |         30 |        438 |     14.6x |   236.00ns |  16.9x |     127 MIPS |  0.63 |       4.00% |
| rvr-rv64i      |         30 |        440 |     14.7x |   264.33ns |  18.9x |     113 MIPS |  0.40 |       4.00% |

## prime-sieve

*Prime number sieve algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     10.74M |         - |     1.82ms |   1.0x |            - |  1.95 |       0.26% |
| rvr-rv64e      |     17.59M |     20.00M |      1.1x |     2.02ms |   1.1x |    8.69 BIPS |  3.28 |       0.20% |
| rvr-rv64i      |     16.18M |     19.99M |      1.2x |     2.16ms |   1.2x |    7.49 BIPS |  3.09 |       0.20% |
| rvr-rv32e      |     23.79M |     26.26M |      1.1x |     2.36ms |   1.3x |   10.09 BIPS |  3.68 |       0.18% |
| rvr-rv32i      |     21.36M |     28.87M |      1.4x |     2.62ms |   1.4x |    8.16 BIPS |  3.64 |       0.18% |

## pinky

*NES emulator (cycle-accurate) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     28.53M |         - |     2.54ms |   1.0x |            - |  3.72 |       1.48% |
| rvr-rv64e      |     30.81M |     44.00M |      1.4x |     3.30ms |   1.3x |    9.35 BIPS |  4.43 |       1.71% |
| rvr-rv32e      |     31.90M |     44.69M |      1.4x |     3.42ms |   1.3x |    9.34 BIPS |  4.34 |       1.69% |
| rvr-rv32i      |     31.12M |     45.21M |      1.5x |     3.46ms |   1.4x |    9.01 BIPS |  4.35 |       1.67% |
| rvr-rv64i      |     30.88M |     46.11M |      1.5x |     3.54ms |   1.4x |    8.72 BIPS |  4.32 |       1.73% |

## memset

*Memory set operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    327.87K |         - |    83.92us |   1.0x |            - |  1.29 |       0.01% |
| rvr-rv64i      |      1.57M |      3.15M |      2.0x |   226.02us |   2.7x |    6.96 BIPS |  4.61 |       0.00% |
| rvr-rv64e      |      1.57M |      3.15M |      2.0x |   227.46us |   2.7x |    6.92 BIPS |  4.60 |       0.00% |
| rvr-rv32e      |      3.15M |      6.29M |      2.0x |   451.28us |   5.4x |    6.97 BIPS |  4.61 |       0.00% |
| rvr-rv32i      |      3.15M |      6.29M |      2.0x |   451.63us |   5.4x |    6.97 BIPS |  4.61 |       0.00% |

## reth

*Reth block validator | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |      1.48B |         - |   134.85ms |   1.0x |            - |  3.72 |       2.10% |
| rvr-rv64e      |      3.36B |      3.49B |      1.0x |   235.72ms |   1.7x |   14.25 BIPS |  4.90 |       1.22% |
| rvr-rv64i      |      2.88B |      3.39B |      1.2x |   243.02ms |   1.8x |   11.87 BIPS |  4.60 |       1.20% |
| rvr-rv32i      |      7.27B |      9.36B |      1.3x |   580.37ms |   4.3x |   12.53 BIPS |  5.34 |       0.77% |
| rvr-rv32e      |      8.55B |      9.81B |      1.1x |   641.37ms |   4.8x |   13.33 BIPS |  5.06 |       0.81% |

