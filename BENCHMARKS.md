# Benchmark Results

## System Information

| Property     | Value                                             |
|--------------|---------------------------------------------------|
| Architecture | aarch64                                           |
| Clang        | clang version 21.1.8 (Fedora 21.1.8-4.fc43)       |
| Rust         | rustc 1.95.0-nightly (873d4682c 2026-01-25)       |
| OS           | Fedora Linux Asahi Remix 43 (Workstation Edition) |
| Date         | 2026-01-28 18:10:23                               |

## towers

*Towers of Hanoi (recursive) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   361.00ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |      8.44K |     28.73K |      3.4x |     8.82us |  24.4x |    1.17 BIPS |  5.08 |       0.34% |
| rvr-rv32i      |      9.56K |     28.37K |      3.0x |     9.25us |  25.6x |    1.33 BIPS |  5.72 |       0.26% |
| rvr-rv64e      |      8.44K |     28.73K |      3.4x |     9.33us |  25.9x |    1.14 BIPS |  4.40 |       0.37% |
| rvr-rv32e      |      9.56K |     28.37K |      3.0x |     9.57us |  26.5x |    1.30 BIPS |  4.87 |       0.28% |

## qsort

*Quick sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    69.08us |   1.0x |            - |     - |           - |
| rvr-rv64e      |    228.90K |    467.73K |      2.0x |    79.06us |   1.1x |    2.96 BIPS |  2.53 |       6.70% |
| rvr-rv64i      |    228.90K |    467.73K |      2.0x |    79.31us |   1.1x |    2.97 BIPS |  2.58 |       6.66% |
| rvr-rv32e      |    228.99K |    452.84K |      2.0x |    81.88us |   1.2x |    2.85 BIPS |  2.28 |       8.01% |
| rvr-rv32i      |    228.99K |    452.84K |      2.0x |    82.03us |   1.2x |    2.84 BIPS |  2.23 |       8.25% |

## rsort

*Radix sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |     6.62us |   1.0x |            - |     - |           - |
| rvr-rv32i      |    367.88K |    558.01K |      1.5x |    45.99us |   6.9x |    8.06 BIPS |  4.62 |       0.06% |
| rvr-rv32e      |    367.88K |    558.01K |      1.5x |    46.01us |   6.9x |    8.07 BIPS |  4.67 |       0.05% |
| rvr-rv64i      |    368.39K |    638.87K |      1.7x |    49.00us |   7.4x |    7.56 BIPS |  4.96 |       0.05% |
| rvr-rv64e      |    368.39K |    638.87K |      1.7x |    49.61us |   7.5x |    7.46 BIPS |  5.01 |       0.06% |

## median

*Median filter | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   805.67ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |     12.86K |     30.37K |      2.4x |     9.26us |  11.5x |    1.63 BIPS |  3.94 |       0.25% |
| rvr-rv64i      |     12.86K |     30.37K |      2.4x |     9.71us |  12.1x |    1.55 BIPS |  3.85 |       0.31% |
| rvr-rv32i      |     12.97K |     30.58K |      2.4x |    10.10us |  12.5x |    1.55 BIPS |  3.82 |       0.28% |
| rvr-rv32e      |     12.97K |     30.58K |      2.4x |    10.33us |  12.8x |    1.57 BIPS |  3.46 |       0.26% |

## multiply

*Software multiply | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   416.33ns |   1.0x |            - |     - |           - |
| rvr-rv32i      |     56.92K |     33.90K |      0.6x |     8.69us |  20.9x |    7.64 BIPS |  5.03 |       0.43% |
| rvr-rv32e      |     56.92K |     33.90K |      0.6x |     8.90us |  21.4x |    7.84 BIPS |  5.92 |       0.51% |
| rvr-rv64e      |     55.62K |     72.07K |      1.3x |    11.19us |  26.9x |    5.31 BIPS |  4.53 |       0.37% |
| rvr-rv64i      |     55.62K |     72.07K |      1.3x |    11.35us |  27.3x |    5.36 BIPS |  4.79 |       0.16% |

## vvadd

*Vector-vector addition | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    97.33ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |      7.84K |     15.33K |      2.0x |     7.06us |  72.5x |    1.32 BIPS |  4.73 |       0.92% |
| rvr-rv64e      |      7.84K |     15.33K |      2.0x |     7.26us |  74.6x |    1.29 BIPS |  4.51 |       0.53% |
| rvr-rv32i      |      8.81K |     17.73K |      2.0x |     7.54us |  77.5x |    1.38 BIPS |  4.59 |       0.28% |
| rvr-rv32e      |      8.81K |     17.73K |      2.0x |     7.92us |  81.3x |    1.42 BIPS |  5.38 |       0.62% |

## memcpy

*Memory copy operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   750.33ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |     28.88K |     52.35K |      1.8x |     9.14us |  12.2x |    3.63 BIPS |  6.48 |       0.13% |
| rvr-rv64i      |     28.88K |     52.35K |      1.8x |     9.46us |  12.6x |    3.57 BIPS |  6.54 |       0.12% |
| rvr-rv32e      |     33.81K |     64.27K |      1.9x |     9.89us |  13.2x |    3.89 BIPS |  6.10 |       0.13% |
| rvr-rv32i      |     33.81K |     64.27K |      1.9x |    10.00us |  13.3x |    3.87 BIPS |  6.35 |       0.06% |

## dhrystone

*Classic Dhrystone benchmark | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    10.36us |   1.0x |            - |     - |           - |
| rvr-rv64e      |    198.11K |    471.56K |      2.4x |    28.58us |   2.8x |    7.16 BIPS |  7.39 |       0.02% |
| rvr-rv64i      |    198.11K |    471.56K |      2.4x |    29.68us |   2.9x |    6.83 BIPS |  7.26 |       0.03% |
| rvr-rv32i      |    210.71K |    470.64K |      2.2x |    32.56us |   3.1x |    6.78 BIPS |  6.79 |       0.03% |
| rvr-rv32e      |    210.71K |    470.64K |      2.2x |    32.56us |   3.1x |    6.77 BIPS |  6.80 |       0.02% |

## fib

*Fibonacci (recursive tail-call) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.67ms |   1.0x |   15.12 BIPS |  5.00 |       0.00% |
| host           |          - |      1.28B |         - |    84.69ms |   1.0x |            - |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.70ms |   1.0x |   15.11 BIPS |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    85.44ms |   1.0x |            - |  4.99 |       0.01% |
| libriscv-rv64i |          - |      1.37B |         - |    96.60ms |   1.1x |            - |  4.76 |       0.19% |
| rvr-rv32e      |      1.28B |      1.28B |      1.0x |   131.92ms |   1.6x |    9.70 BIPS |  3.21 |       0.00% |
| rvr-rv32i      |      1.28B |      1.28B |      1.0x |   131.93ms |   1.6x |    9.70 BIPS |  3.21 |       0.00% |
| libriscv-rv32e |          - |      1.28B |         - |   172.50ms |   2.0x |            - |  2.47 |       0.01% |
| libriscv-rv32i |          - |      1.37B |         - |   184.21ms |   2.2x |            - |  2.49 |       0.20% |

## fib-asm

*Fibonacci (hand-written assembly) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.64ms |      - |   15.12 BIPS |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.64ms |      - |   15.12 BIPS |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    85.42ms |      - |            - |  4.99 |       0.01% |
| libriscv-rv64i |          - |      1.34B |         - |    93.68ms |      - |            - |  4.81 |       0.15% |

## coremark

*CoreMark CPU benchmark (EEMBC) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    111.78B |         - |     12.70s |   1.0x |            - |  2.91 |       0.18% |
| rvr-rv32i      |    109.57B |    156.25B |      1.4x |     15.90s |   1.3x |    6.89 BIPS |  3.25 |       0.17% |
| rvr-rv32e      |    109.57B |    156.25B |      1.4x |     15.90s |   1.3x |    6.89 BIPS |  3.25 |       0.17% |
| rvr-rv64e      |    134.05B |    174.62B |      1.3x |     17.84s |   1.4x |    7.52 BIPS |  3.24 |       0.16% |
| rvr-rv64i      |    134.05B |    174.62B |      1.3x |     17.84s |   1.4x |    7.52 BIPS |  3.24 |       0.16% |
| libriscv-rv32e |          - |    422.26B |         - |     29.14s |   2.3x |            - |  4.79 |       0.09% |
| libriscv-rv32i |          - |    445.05B |         - |     30.95s |   2.4x |            - |  4.76 |       0.12% |
| libriscv-rv64e |          - |    585.58B |         - |     38.10s |   3.0x |            - |  5.08 |       0.11% |
| libriscv-rv64i |          - |    603.07B |         - |     39.57s |   3.1x |            - |  5.04 |       0.14% |

## minimal

*Minimal function call overhead | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |        184 |         - |    27.67ns |   1.0x |            - |  0.68 |      12.20% |
| rvr-rv32i      |         30 |        434 |     14.5x |   153.00ns |   5.5x |     196 MIPS |  1.25 |       4.00% |
| rvr-rv64i      |         30 |        436 |     14.5x |   153.00ns |   5.5x |     196 MIPS |  1.24 |       4.00% |
| rvr-rv32e      |         30 |        426 |     14.2x |   180.33ns |   6.5x |     166 MIPS |  1.25 |       4.00% |
| rvr-rv64e      |         30 |        427 |     14.2x |   194.67ns |   7.0x |     154 MIPS |  1.21 |       4.00% |

## prime-sieve

*Prime number sieve algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     10.74M |         - |     1.83ms |   1.0x |            - |  1.95 |       0.25% |
| rvr-rv64e      |     17.59M |     16.85M |      1.0x |     2.00ms |   1.1x |    8.79 BIPS |  2.79 |       0.21% |
| rvr-rv64i      |     16.18M |     16.77M |      1.0x |     2.10ms |   1.2x |    7.69 BIPS |  2.64 |       0.21% |
| rvr-rv32e      |     23.79M |     20.82M |      0.9x |     2.23ms |   1.2x |   10.67 BIPS |  3.08 |       0.20% |
| rvr-rv32i      |     21.36M |     24.06M |      1.1x |     2.48ms |   1.4x |    8.60 BIPS |  3.21 |       0.19% |

## pinky

*NES emulator (cycle-accurate) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     28.53M |         - |     2.53ms |   1.0x |            - |  3.73 |       1.47% |
| rvr-rv64e      |     30.81M |     41.56M |      1.3x |     3.23ms |   1.3x |    9.55 BIPS |  4.25 |       1.69% |
| rvr-rv32e      |     31.90M |     42.12M |      1.3x |     3.32ms |   1.3x |    9.61 BIPS |  4.19 |       1.69% |
| rvr-rv64i      |     30.88M |     42.92M |      1.4x |     3.33ms |   1.3x |    9.27 BIPS |  4.25 |       1.72% |
| rvr-rv32i      |     31.12M |     42.32M |      1.4x |     3.34ms |   1.3x |    9.30 BIPS |  4.18 |       1.66% |

## memset

*Memory set operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    327.87K |         - |    83.79us |   1.0x |            - |  1.29 |       0.01% |
| rvr-rv64i      |      1.57M |      3.15M |      2.0x |   224.78us |   2.7x |    7.00 BIPS |  4.61 |       0.00% |
| rvr-rv64e      |      1.57M |      3.15M |      2.0x |   226.10us |   2.7x |    6.96 BIPS |  4.61 |       0.00% |
| rvr-rv32e      |      3.15M |      6.29M |      2.0x |   449.50us |   5.4x |    7.00 BIPS |  4.61 |       0.00% |
| rvr-rv32i      |      3.15M |      6.29M |      2.0x |   451.25us |   5.4x |    6.97 BIPS |  4.61 |       0.00% |

## reth

*Reth block validator | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |      1.47B |         - |   130.90ms |   1.0x |            - |  3.79 |       2.10% |
| rvr-rv64e      |      3.36B |      2.85B |      0.8x |   202.48ms |   1.5x |   16.59 BIPS |  4.66 |       1.24% |
| rvr-rv64i      |      2.88B |      2.99B |      1.0x |   220.82ms |   1.7x |   13.06 BIPS |  4.49 |       1.20% |
| rvr-rv32i      |      7.27B |      7.99B |      1.1x |   524.99ms |   4.0x |   13.85 BIPS |  5.03 |       0.77% |
| rvr-rv32e      |      8.55B |      7.84B |      0.9x |   538.12ms |   4.1x |   15.89 BIPS |  4.82 |       0.81% |

