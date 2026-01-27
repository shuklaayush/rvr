# Benchmark Results

## System Information

| Property     | Value                                             |
|--------------|---------------------------------------------------|
| Architecture | aarch64                                           |
| Clang        | clang version 21.1.8 (Fedora 21.1.8-4.fc43)       |
| Rust         | rustc 1.95.0-nightly (873d4682c 2026-01-25)       |
| OS           | Fedora Linux Asahi Remix 43 (Workstation Edition) |
| Date         | 2026-01-27 20:30:56                               |

## towers

*Towers of Hanoi (recursive) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   375.00ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |      8.44K |     30.77K |      3.6x |    10.22us |  27.3x |     977 MIPS |  2.99 |       0.37% |
| rvr-rv64i      |      8.44K |     30.77K |      3.6x |    11.93us |  31.8x |     796 MIPS |  2.95 |       0.26% |
| rvr-rv32e      |      9.56K |     29.05K |      3.0x |    11.96us |  31.9x |     940 MIPS |  1.59 |       0.23% |
| rvr-rv32i      |      9.56K |     29.05K |      3.0x |    12.21us |  32.6x |     876 MIPS |  2.66 |       0.26% |

## qsort

*Quick sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    67.11us |   1.0x |            - |     - |           - |
| rvr-rv32i      |    228.99K |    455.73K |      2.0x |    78.85us |   1.2x |    3.01 BIPS |  2.62 |       6.21% |
| rvr-rv32e      |    228.99K |    455.73K |      2.0x |    80.18us |   1.2x |    2.95 BIPS |  2.55 |       6.20% |
| rvr-rv64e      |    228.90K |    471.05K |      2.1x |    83.21us |   1.2x |    2.79 BIPS |  2.32 |       7.83% |
| rvr-rv64i      |    228.90K |    471.05K |      2.1x |    84.17us |   1.3x |    2.76 BIPS |  2.29 |       7.85% |

## rsort

*Radix sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |     6.64us |   1.0x |            - |     - |           - |
| rvr-rv32e      |    367.88K |    558.80K |      1.5x |    45.71us |   6.9x |    8.12 BIPS |  4.78 |       0.06% |
| rvr-rv32i      |    367.88K |    558.80K |      1.5x |    46.71us |   7.0x |    7.92 BIPS |  4.81 |       0.06% |
| rvr-rv64e      |    368.39K |    643.04K |      1.7x |    51.88us |   7.8x |    7.15 BIPS |  4.74 |       0.06% |
| rvr-rv64i      |    368.39K |    643.04K |      1.7x |    53.21us |   8.0x |    6.94 BIPS |  4.63 |       0.06% |

## median

*Median filter | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   653.00ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |     12.86K |     30.51K |      2.4x |     9.92us |  15.2x |    1.54 BIPS |  3.74 |       0.27% |
| rvr-rv32i      |     12.97K |     30.42K |      2.3x |    10.36us |  15.9x |    1.53 BIPS |  3.60 |       0.21% |
| rvr-rv32e      |     12.97K |     30.42K |      2.3x |    11.17us |  17.1x |    1.33 BIPS |  2.90 |       0.24% |
| rvr-rv64e      |     12.86K |     30.51K |      2.4x |    13.28us |  20.3x |     996 MIPS |  1.83 |       0.30% |

## multiply

*Software multiply | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   389.00ns |   1.0x |            - |     - |           - |
| rvr-rv32e      |     56.92K |     33.73K |      0.6x |     9.19us |  23.6x |    7.47 BIPS |  5.55 |       0.51% |
| rvr-rv64i      |     55.62K |     72.24K |      1.3x |    11.69us |  30.1x |    5.10 BIPS |  4.42 |       0.18% |
| rvr-rv32i      |     56.92K |     33.73K |      0.6x |    12.21us |  31.4x |    5.29 BIPS |  1.72 |       0.99% |
| rvr-rv64e      |     55.62K |     72.24K |      1.3x |    12.47us |  32.1x |    4.69 BIPS |  3.98 |       0.20% |

## vvadd

*Vector-vector addition | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    83.33ns |   1.0x |            - |     - |           - |
| rvr-rv64i      |      7.84K |     15.48K |      2.0x |     6.90us |  82.8x |    1.33 BIPS |  5.07 |       0.30% |
| rvr-rv32i      |      8.81K |     17.57K |      2.0x |     8.04us |  96.5x |    1.37 BIPS |  5.01 |       0.53% |
| rvr-rv32e      |      8.81K |     17.57K |      2.0x |     8.35us | 100.2x |    1.23 BIPS |  3.83 |       0.25% |
| rvr-rv64e      |      7.84K |     15.48K |      2.0x |    10.33us | 124.0x |     797 MIPS |  2.34 |       0.69% |

## memcpy

*Memory copy operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |   708.67ns |   1.0x |            - |     - |           - |
| rvr-rv64e      |     28.88K |     52.97K |      1.8x |     9.43us |  13.3x |    3.38 BIPS |  5.25 |       0.18% |
| rvr-rv32i      |     33.81K |     62.10K |      1.8x |    10.29us |  14.5x |    3.65 BIPS |  5.60 |       0.06% |
| rvr-rv64i      |     28.88K |     52.97K |      1.8x |    10.33us |  14.6x |    3.04 BIPS |  5.76 |       0.27% |
| rvr-rv32e      |     33.81K |     62.10K |      1.8x |    10.44us |  14.7x |    3.55 BIPS |  5.06 |       0.14% |

## dhrystone

*Classic Dhrystone benchmark | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |          - |         - |    10.74us |   1.0x |            - |     - |           - |
| rvr-rv64e      |    198.11K |    497.05K |      2.5x |    31.08us |   2.9x |    6.52 BIPS |  6.96 |       0.03% |
| rvr-rv64i      |    198.11K |    497.05K |      2.5x |    31.26us |   2.9x |    6.45 BIPS |  7.00 |       0.03% |
| rvr-rv32e      |    210.71K |    461.95K |      2.2x |    32.70us |   3.0x |    6.70 BIPS |  6.92 |       0.03% |
| rvr-rv32i      |    210.71K |    461.95K |      2.2x |    37.00us |   3.4x |    5.85 BIPS |  6.06 |       0.03% |

## fib

*Fibonacci (recursive tail-call) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.73ms |   1.0x |   15.11 BIPS |  5.00 |       0.00% |
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.83ms |   1.0x |   15.09 BIPS |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    85.65ms |   1.0x |            - |  4.98 |       0.01% |
| host           |          - |      1.28B |         - |    85.71ms |   1.0x |            - |  5.00 |       0.00% |
| libriscv-rv64i |          - |      1.28B |         - |    85.83ms |   1.0x |            - |  4.98 |       0.01% |
| rvr-rv32e      |      1.28B |      1.28B |      1.0x |   132.05ms |   1.5x |    9.69 BIPS |  3.21 |       0.00% |
| rvr-rv32i      |      1.28B |      1.28B |      1.0x |   132.10ms |   1.5x |    9.69 BIPS |  3.21 |       0.00% |
| libriscv-rv32e |          - |      1.28B |         - |   172.80ms |   2.0x |            - |  2.46 |       0.01% |
| libriscv-rv32i |          - |      1.28B |         - |   172.88ms |   2.0x |            - |  2.46 |       0.01% |

## fib-asm

*Fibonacci (hand-written assembly) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.81ms |      - |   15.09 BIPS |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.86ms |      - |   15.08 BIPS |  5.00 |       0.00% |
| libriscv-rv64i |          - |      1.28B |         - |    85.79ms |      - |            - |  4.98 |       0.01% |
| libriscv-rv64e |          - |      1.28B |         - |    85.79ms |      - |            - |  4.98 |       0.01% |

## coremark

*CoreMark CPU benchmark (EEMBC) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    111.78B |         - |     12.73s |   1.0x |            - |  2.91 |       0.18% |
| rvr-rv32i      |    109.57B |    157.71B |      1.4x |     16.02s |   1.3x |    6.84 BIPS |  3.26 |       0.17% |
| rvr-rv32e      |    109.57B |    157.71B |      1.4x |     16.02s |   1.3x |    6.84 BIPS |  3.26 |       0.17% |
| rvr-rv64e      |    134.05B |    175.57B |      1.3x |     18.01s |   1.4x |    7.44 BIPS |  3.23 |       0.16% |
| rvr-rv64i      |    134.05B |    175.57B |      1.3x |     18.02s |   1.4x |    7.44 BIPS |  3.23 |       0.16% |
| libriscv-rv32i |          - |    422.26B |         - |     29.11s |   2.3x |            - |  4.80 |       0.09% |
| libriscv-rv32e |          - |    422.26B |         - |     29.13s |   2.3x |            - |  4.80 |       0.09% |
| libriscv-rv64i |          - |    585.58B |         - |     38.12s |   3.0x |            - |  5.08 |       0.11% |
| libriscv-rv64e |          - |    585.58B |         - |     38.14s |   3.0x |            - |  5.08 |       0.11% |

## minimal

*Minimal function call overhead | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |        184 |         - |    27.67ns |   1.0x |            - |  0.57 |       9.76% |
| rvr-rv64i      |         30 |        440 |     14.7x |   138.67ns |   5.0x |     216 MIPS |  1.24 |       4.00% |
| rvr-rv32e      |         30 |        429 |     14.3x |   153.00ns |   5.5x |     196 MIPS |  1.24 |       4.00% |
| rvr-rv64e      |         30 |        430 |     14.3x |   194.67ns |   7.0x |     154 MIPS |  0.40 |       4.00% |
| rvr-rv32i      |         30 |        438 |     14.6x |   208.33ns |   7.5x |     144 MIPS |  1.26 |       4.00% |

## prime-sieve

*Prime number sieve algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     10.74M |         - |     1.82ms |   1.0x |            - |  1.95 |       0.26% |
| rvr-rv64e      |     17.59M |     20.00M |      1.1x |     2.02ms |   1.1x |    8.72 BIPS |  3.27 |       0.20% |
| rvr-rv64i      |     16.18M |     19.99M |      1.2x |     2.13ms |   1.2x |    7.60 BIPS |  3.11 |       0.20% |
| rvr-rv32e      |     23.79M |     26.26M |      1.1x |     2.34ms |   1.3x |   10.16 BIPS |  3.72 |       0.18% |
| rvr-rv32i      |     21.36M |     28.87M |      1.4x |     2.60ms |   1.4x |    8.21 BIPS |  3.67 |       0.18% |

## pinky

*NES emulator (cycle-accurate) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |     28.53M |         - |     2.54ms |   1.0x |            - |  3.72 |       1.48% |
| rvr-rv64e      |     30.81M |     44.00M |      1.4x |     3.29ms |   1.3x |    9.36 BIPS |  4.42 |       1.72% |
| rvr-rv32e      |     31.90M |     44.69M |      1.4x |     3.38ms |   1.3x |    9.43 BIPS |  4.38 |       1.65% |
| rvr-rv32i      |     31.12M |     45.21M |      1.5x |     3.44ms |   1.4x |    9.04 BIPS |  4.34 |       1.67% |
| rvr-rv64i      |     30.88M |     46.11M |      1.5x |     3.52ms |   1.4x |    8.76 BIPS |  4.33 |       1.72% |

## memset

*Memory set operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    327.87K |         - |    83.99us |   1.0x |            - |  1.28 |       0.01% |
| rvr-rv64e      |      1.57M |      3.15M |      2.0x |   225.50us |   2.7x |    6.98 BIPS |  4.61 |       0.00% |
| rvr-rv64i      |      1.57M |      3.15M |      2.0x |   228.78us |   2.7x |    6.88 BIPS |  4.61 |       0.00% |
| rvr-rv32i      |      3.15M |      6.29M |      2.0x |   450.27us |   5.4x |    6.99 BIPS |  4.61 |       0.00% |
| rvr-rv32e      |      3.15M |      6.29M |      2.0x |   450.60us |   5.4x |    6.98 BIPS |  4.61 |       0.00% |

## reth

*Reth block validator | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |      1.48B |         - |   134.18ms |   1.0x |            - |  3.78 |       2.10% |
| rvr-rv64e      |      3.36B |      3.49B |      1.0x |   233.02ms |   1.7x |   14.42 BIPS |  4.97 |       1.23% |
| rvr-rv64i      |      2.88B |      3.39B |      1.2x |   242.24ms |   1.8x |   11.91 BIPS |  4.63 |       1.20% |
| rvr-rv32i      |      7.27B |      9.36B |      1.3x |   580.73ms |   4.3x |   12.52 BIPS |  5.35 |       0.77% |
| rvr-rv32e      |      8.55B |      9.81B |      1.1x |   637.82ms |   4.8x |   13.40 BIPS |  5.08 |       0.81% |

