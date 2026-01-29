# Benchmark Results

## System Information

| Property     | Value                                             |
|--------------|---------------------------------------------------|
| Architecture | aarch64                                           |
| Clang        | clang version 21.1.8 (Fedora 21.1.8-4.fc43)       |
| Rust         | rustc 1.95.0-nightly (873d4682c 2026-01-25)       |
| OS           | Fedora Linux Asahi Remix 43 (Workstation Edition) |
| Date         | 2026-01-29 01:40:17                               |

## towers

*Towers of Hanoi (recursive) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   375.00ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |      8.44K |     27.98K |      3.3x |     8.85us |  23.6x |    1.17 BIPS |  5.27 |       0.24% |
| rvr-rv32e      |      9.56K |     27.05K |      2.8x |     9.50us |  25.3x |    1.29 BIPS |  4.85 |       0.21% |
| rvr-rv32i      |      9.56K |     27.05K |      2.8x |     9.93us |  26.5x |    1.27 BIPS |  4.99 |       0.19% |
| rvr-rv64e      |      8.44K |     27.98K |      3.3x |    10.51us |  28.0x |     893 MIPS |  2.70 |       0.29% |

## qsort

*Quick sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    63.64us |   1.0x |            - |     - |           - |
| rvr-rv32e      |    228.99K |    447.51K |      2.0x |    77.36us |   1.2x |    3.07 BIPS |  2.60 |       6.15% |
| rvr-rv64i      |    228.90K |    461.48K |      2.0x |    81.82us |   1.3x |    2.84 BIPS |  2.32 |       8.04% |
| rvr-rv32i      |    228.99K |    447.51K |      2.0x |    81.95us |   1.3x |    2.85 BIPS |  2.28 |       7.13% |
| rvr-rv64e      |    228.90K |    461.48K |      2.0x |    84.22us |   1.3x |    2.79 BIPS |  2.33 |       7.94% |

## rsort

*Radix sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |     6.60us |   1.0x |            - |     - |           - |
| rvr-rv32e      |    367.88K |    541.24K |      1.5x |    41.93us |   6.4x |    8.86 BIPS |  5.22 |       0.07% |
| rvr-rv32i      |    367.88K |    541.24K |      1.5x |    42.00us |   6.4x |    8.86 BIPS |  5.16 |       0.06% |
| rvr-rv64e      |    368.39K |    630.42K |      1.7x |    48.60us |   7.4x |    7.72 BIPS |  5.20 |       0.06% |
| rvr-rv64i      |    368.39K |    630.42K |      1.7x |    50.18us |   7.6x |    7.43 BIPS |  5.11 |       0.06% |

## median

*Median filter | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   750.00ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |     12.86K |     27.01K |      2.1x |     8.96us |  11.9x |    1.65 BIPS |  3.36 |       0.28% |
| rvr-rv64i      |     12.86K |     27.01K |      2.1x |     9.08us |  12.1x |    1.63 BIPS |  3.50 |       0.28% |
| rvr-rv32i      |     12.97K |     30.24K |      2.3x |     9.96us |  13.3x |    1.60 BIPS |  3.64 |       0.24% |
| rvr-rv32e      |     12.97K |     30.24K |      2.3x |    11.18us |  14.9x |    1.36 BIPS |  3.73 |       0.22% |

## multiply

*Software multiply | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   403.00ns |   1.0x |            - |     - |           - |
| rvr-rv32e      |     56.92K |     33.29K |      0.6x |     8.07us |  20.0x |    8.16 BIPS |  5.50 |       0.73% |
| rvr-rv32i      |     56.92K |     33.29K |      0.6x |     8.40us |  20.9x |    7.94 BIPS |  5.88 |       0.77% |
| rvr-rv64i      |     55.62K |     71.69K |      1.3x |    11.35us |  28.2x |    5.13 BIPS |  4.07 |       0.21% |
| rvr-rv64e      |     55.62K |     71.69K |      1.3x |    11.37us |  28.2x |    5.24 BIPS |  4.43 |       0.22% |

## vvadd

*Vector-vector addition | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    83.33ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |      7.84K |     15.15K |      1.9x |     6.86us |  82.3x |    1.31 BIPS |  4.97 |       0.30% |
| rvr-rv64i      |      7.84K |     15.15K |      1.9x |     6.99us |  83.8x |    1.31 BIPS |  4.45 |       1.06% |
| rvr-rv32e      |      8.81K |     17.39K |      2.0x |     7.40us |  88.8x |    1.45 BIPS |  5.40 |       0.43% |
| rvr-rv32i      |      8.81K |     17.39K |      2.0x |     8.68us | 104.2x |    1.36 BIPS |  5.26 |       0.81% |

## memcpy

*Memory copy operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   764.00ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |     28.88K |     52.17K |      1.8x |     8.54us |  11.2x |    3.72 BIPS |  6.51 |       0.09% |
| rvr-rv64e      |     28.88K |     52.17K |      1.8x |     8.76us |  11.5x |    3.67 BIPS |  6.38 |       0.17% |
| rvr-rv32i      |     33.81K |     61.91K |      1.8x |     9.78us |  12.8x |    3.99 BIPS |  6.56 |       0.10% |
| rvr-rv32e      |     33.81K |     61.91K |      1.8x |    11.36us |  14.9x |    3.23 BIPS |  4.40 |       0.06% |

## dhrystone

*Classic Dhrystone benchmark | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    10.54us |   1.0x |            - |     - |           - |
| rvr-rv64e      |    198.11K |    445.64K |      2.2x |    27.21us |   2.6x |    7.54 BIPS |  7.52 |       0.03% |
| rvr-rv64i      |    198.11K |    445.64K |      2.2x |    27.64us |   2.6x |    7.34 BIPS |  7.01 |       0.03% |
| rvr-rv32i      |    210.71K |    449.31K |      2.1x |    33.01us |   3.1x |    6.65 BIPS |  6.19 |       0.03% |
| rvr-rv32e      |    210.71K |    449.31K |      2.1x |    33.06us |   3.1x |    6.62 BIPS |  6.32 |       0.03% |

## fib

*Fibonacci (recursive tail-call) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.63ms |   1.0x |   15.12 BIPS |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.69ms |   1.0x |   15.11 BIPS |  5.00 |       0.00% |
| host           |          - |      1.28B |         - |    84.69ms |   1.0x |            - |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    85.38ms |   1.0x |            - |  4.99 |       0.01% |
| libriscv-rv64i |          - |      1.28B |         - |    85.39ms |   1.0x |            - |  4.99 |       0.01% |
| rvr-rv32i      |      1.28B |      1.28B |      1.0x |   131.88ms |   1.6x |    9.71 BIPS |  3.21 |       0.00% |
| rvr-rv32e      |      1.28B |      1.28B |      1.0x |   131.88ms |   1.6x |    9.71 BIPS |  3.21 |       0.00% |
| libriscv-rv32e |          - |      1.28B |         - |   172.38ms |   2.0x |            - |  2.47 |       0.01% |
| libriscv-rv32i |          - |      1.28B |         - |   172.96ms |   2.0x |            - |  2.47 |       0.01% |

## fib-asm

*Fibonacci (hand-written assembly) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.64ms |      - |   15.12 BIPS |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.65ms |      - |   15.12 BIPS |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    85.49ms |      - |            - |  4.98 |       0.01% |
| libriscv-rv64i |          - |      1.28B |         - |    85.65ms |      - |            - |  4.98 |       0.01% |

## coremark

*CoreMark CPU benchmark (EEMBC) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    111.78B |         - |     12.74s |   1.0x |            - |  2.90 |       0.18% |
| rvr-rv32e      |    109.57B |    156.39B |      1.4x |     15.48s |   1.2x |    7.08 BIPS |  3.34 |       0.17% |
| rvr-rv32i      |    109.57B |    156.39B |      1.4x |     15.48s |   1.2x |    7.08 BIPS |  3.34 |       0.17% |
| rvr-rv64e      |    134.05B |    176.14B |      1.3x |     17.54s |   1.4x |    7.64 BIPS |  3.32 |       0.21% |
| rvr-rv64i      |    134.05B |    176.14B |      1.3x |     17.55s |   1.4x |    7.64 BIPS |  3.32 |       0.21% |
| libriscv-rv32i |          - |    422.26B |         - |     29.11s |   2.3x |            - |  4.80 |       0.09% |
| libriscv-rv32e |          - |    422.26B |         - |     29.17s |   2.3x |            - |  4.79 |       0.09% |
| libriscv-rv64e |          - |    585.58B |         - |     38.09s |   3.0x |            - |  5.08 |       0.11% |
| libriscv-rv64i |          - |    585.58B |         - |     38.12s |   3.0x |            - |  5.08 |       0.11% |

## minimal

*Minimal function call overhead | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |        184 |         - |    28.00ns |   1.0x |            - |  0.61 |      14.63% |
| rvr-rv64i      |         30 |        431 |     14.4x |   125.00ns |   4.5x |     240 MIPS |  1.20 |       4.00% |
| rvr-rv32i      |         30 |        431 |     14.4x |   152.33ns |   5.4x |     197 MIPS |  1.22 |       4.00% |
| rvr-rv32e      |         30 |        422 |     14.1x |   153.00ns |   5.5x |     196 MIPS |  1.19 |       4.00% |
| rvr-rv64e      |         30 |        422 |     14.1x |   153.00ns |   5.5x |     196 MIPS |  1.17 |       4.00% |

## prime-sieve

*Prime number sieve algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     10.74M |         - |     1.82ms |   1.0x |            - |  1.95 |       0.26% |
| rvr-rv64i      |     16.18M |     17.51M |      1.1x |     1.91ms |   1.1x |    8.47 BIPS |  3.04 |       0.21% |
| rvr-rv32i      |     21.36M |     17.20M |      0.8x |     1.96ms |   1.1x |   10.92 BIPS |  2.91 |       0.19% |
| rvr-rv64e      |     17.59M |     16.85M |      1.0x |     2.00ms |   1.1x |    8.78 BIPS |  2.79 |       0.21% |
| rvr-rv32e      |     23.79M |     20.82M |      0.9x |     2.23ms |   1.2x |   10.66 BIPS |  3.08 |       0.19% |

## pinky

*NES emulator (cycle-accurate) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     28.53M |         - |     2.54ms |   1.0x |            - |  3.72 |       1.48% |
| rvr-rv64e      |     30.81M |     41.56M |      1.3x |     3.22ms |   1.3x |    9.56 BIPS |  4.27 |       1.69% |
| rvr-rv64i      |     30.88M |     42.26M |      1.4x |     3.27ms |   1.3x |    9.44 BIPS |  4.26 |       1.75% |
| rvr-rv32i      |     31.12M |     41.27M |      1.3x |     3.29ms |   1.3x |    9.47 BIPS |  4.16 |       1.66% |
| rvr-rv32e      |     31.90M |     42.14M |      1.3x |     3.32ms |   1.3x |    9.61 BIPS |  4.20 |       1.70% |

## memset

*Memory set operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    327.87K |         - |    83.92us |   1.0x |            - |  1.29 |       0.01% |
| rvr-rv64e      |      1.57M |      3.15M |      2.0x |   224.82us |   2.7x |    7.00 BIPS |  4.61 |       0.00% |
| rvr-rv64i      |      1.57M |      3.15M |      2.0x |   224.88us |   2.7x |    6.99 BIPS |  4.61 |       0.00% |
| rvr-rv32i      |      3.15M |      6.29M |      2.0x |   418.52us |   5.0x |    7.52 BIPS |  5.83 |       0.00% |
| rvr-rv32e      |      3.15M |      6.29M |      2.0x |   452.18us |   5.4x |    6.96 BIPS |  4.61 |       0.00% |

## reth

*Reth block validator | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |      1.47B |         - |   136.72ms |   1.0x |            - |  3.63 |       2.10% |
| rvr-rv64i      |      2.88B |      2.84B |      1.0x |   197.17ms |   1.4x |   14.63 BIPS |  4.77 |       1.20% |
| rvr-rv64e      |      3.36B |      2.85B |      0.8x |   206.41ms |   1.5x |   16.27 BIPS |  4.55 |       1.25% |
| rvr-rv32i      |      7.27B |      7.30B |      1.0x |   485.62ms |   3.6x |   14.97 BIPS |  4.97 |       0.77% |
| rvr-rv32e      |      8.55B |      7.84B |      0.9x |   538.83ms |   3.9x |   15.87 BIPS |  4.82 |       0.82% |

