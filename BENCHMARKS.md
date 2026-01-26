# Benchmark Results

## System Information

| Property | Value |
|----------|-------|
| Kernel | Linux 6.17.12-400.asahi.fc43.aarch64+16k |
| Architecture | aarch64 |
| Memory | 30.9 GB |
| OS | Fedora Linux Asahi Remix 43 (Workstation Edition) |
| Rust | rustc 1.94.0-nightly (22c74ba91 2026-01-15) |
| Clang | clang version 21.1.8 (Fedora 21.1.8-1.fc43) |
| Date | 2026-01-26 22:21:19 |

## Configuration

- **Runs per benchmark**: 3
- **Host comparison**: enabled
- **libriscv comparison**: enabled

---

## towers

*Towers of Hanoi (recursive) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv32e      |      9.56K |     29.05K |      3.0x |     9.28us |      - |    1.35 BIPS |  5.86 |       0.14% |
| rvr-rv64e      |      8.44K |     30.77K |      3.6x |    10.96us |      - |     824 MIPS |  3.58 |       0.29% |
| rvr-rv64i      |      8.44K |     30.77K |      3.6x |    11.10us |      - |     843 MIPS |  3.09 |       0.37% |
| rvr-rv32i      |      9.56K |     29.05K |      3.0x |    11.72us |      - |     938 MIPS |  3.05 |       0.23% |

## qsort

*Quick sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv32e      |    228.99K |    455.73K |      2.0x |    80.31us |      - |    2.92 BIPS |  2.44 |       6.41% |
| rvr-rv64e      |    228.90K |    471.05K |      2.1x |    80.50us |      - |    2.89 BIPS |  2.38 |       7.67% |
| rvr-rv64i      |    228.90K |    471.05K |      2.1x |    80.79us |      - |    2.89 BIPS |  2.40 |       7.65% |
| rvr-rv32i      |    228.99K |    455.73K |      2.0x |    81.25us |      - |    2.91 BIPS |  2.58 |       6.19% |

## rsort

*Radix sort algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv32e      |    367.88K |    558.80K |      1.5x |    45.94us |      - |    8.08 BIPS |  4.71 |       0.06% |
| rvr-rv32i      |    367.88K |    558.80K |      1.5x |    48.14us |      - |    7.68 BIPS |  4.74 |       0.06% |
| rvr-rv64i      |    368.39K |    643.04K |      1.7x |    49.86us |      - |    7.42 BIPS |  4.80 |       0.06% |
| rvr-rv64e      |    368.39K |    643.04K |      1.7x |    52.60us |      - |    7.01 BIPS |  4.56 |       0.06% |

## median

*Median filter | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |     12.86K |     30.51K |      2.4x |     9.60us |      - |    1.60 BIPS |  3.94 |       0.15% |
| rvr-rv64i      |     12.86K |     30.51K |      2.4x |    11.79us |      - |    1.16 BIPS |  2.30 |       0.24% |
| rvr-rv32i      |     12.97K |     30.42K |      2.3x |    13.35us |      - |    1.14 BIPS |  2.08 |       0.17% |
| rvr-rv32e      |     12.97K |     30.42K |      2.3x |    15.44us |      - |     849 MIPS |  1.48 |       0.37% |

## multiply

*Software multiply | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv32i      |     56.92K |     33.73K |      0.6x |    10.00us |      - |    6.27 BIPS |  2.56 |       0.77% |
| rvr-rv32e      |     56.92K |     33.73K |      0.6x |    10.03us |      - |    6.37 BIPS |  4.43 |       0.64% |
| rvr-rv64e      |     55.62K |     72.24K |      1.3x |    12.49us |      - |    4.74 BIPS |  4.35 |       0.25% |
| rvr-rv64i      |     55.62K |     72.24K |      1.3x |    12.51us |      - |    4.79 BIPS |  4.68 |       0.22% |

## vvadd

*Vector-vector addition | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      7.84K |     15.48K |      2.0x |     7.94us |      - |    1.12 BIPS |  4.60 |       0.43% |
| rvr-rv64i      |      7.84K |     15.48K |      2.0x |     8.25us |      - |    1.02 BIPS |  3.29 |       0.69% |
| rvr-rv32i      |      8.81K |     17.57K |      2.0x |     8.56us |      - |    1.17 BIPS |  3.95 |       1.43% |
| rvr-rv32e      |      8.81K |     17.57K |      2.0x |     9.39us |      - |    1.02 BIPS |  1.53 |       0.99% |

## memcpy

*Memory copy operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |     28.88K |     52.97K |      1.8x |     8.85us |      - |    3.46 BIPS |  4.92 |       0.05% |
| rvr-rv64i      |     28.88K |     52.97K |      1.8x |    11.21us |      - |    2.68 BIPS |  3.93 |       0.17% |
| rvr-rv32e      |     33.81K |     62.10K |      1.8x |    12.35us |      - |    2.90 BIPS |  3.97 |       0.11% |
| rvr-rv32i      |     33.81K |     62.10K |      1.8x |    12.38us |      - |    2.98 BIPS |  5.48 |       0.12% |

## dhrystone

*Classic Dhrystone benchmark | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |    198.11K |    497.05K |      2.5x |    29.81us |      - |    6.83 BIPS |  7.43 |       0.03% |
| rvr-rv64i      |    198.11K |    497.05K |      2.5x |    31.21us |      - |    6.44 BIPS |  6.93 |       0.02% |
| rvr-rv32i      |    210.71K |    461.95K |      2.2x |    32.39us |      - |    6.72 BIPS |  6.05 |       0.04% |
| rvr-rv32e      |    210.71K |    461.95K |      2.2x |    34.19us |      - |    6.38 BIPS |  5.98 |       0.02% |

## fib

*Fibonacci (recursive tail-call) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.62ms |      - |   15.13 BIPS |  5.00 |       0.00% |
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    85.07ms |      - |   15.05 BIPS |  5.00 |       0.00% |
| libriscv-rv64e |          - |      1.28B |         - |    85.61ms |      - |            - |  4.98 |       0.01% |
| libriscv-rv64i |          - |      1.28B |         - |    85.72ms |      - |            - |  4.98 |       0.01% |
| rvr-rv32e      |      1.28B |      1.28B |      1.0x |   131.89ms |      - |    9.70 BIPS |  3.21 |       0.00% |
| rvr-rv32i      |      1.28B |      1.28B |      1.0x |   131.96ms |      - |    9.70 BIPS |  3.21 |       0.00% |
| libriscv-rv32i |          - |      1.28B |         - |   172.67ms |      - |            - |  2.46 |       0.01% |
| libriscv-rv32e |          - |      1.28B |         - |   172.67ms |      - |            - |  2.46 |       0.01% |

## fib-asm

*Fibonacci (hand-written assembly) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64i      |      1.28B |      1.28B |      1.0x |    84.69ms |      - |   15.11 BIPS |  5.00 |       0.00% |
| rvr-rv64e      |      1.28B |      1.28B |      1.0x |    84.71ms |      - |   15.11 BIPS |  5.00 |       0.00% |
| libriscv-rv64i |          - |      1.28B |         - |    85.60ms |      - |            - |  4.98 |       0.01% |
| libriscv-rv64e |          - |      1.28B |         - |    85.63ms |      - |            - |  4.98 |       0.01% |

## coremark

*CoreMark CPU benchmark (EEMBC) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| host           |          - |    111.78B |         - |     12.72s |   1.0x |            - |  2.91 |       0.18% |
| rvr-rv32e      |    109.57B |    157.71B |      1.4x |     16.00s |   1.3x |    6.85 BIPS |  3.26 |       0.17% |
| rvr-rv32i      |    109.57B |    157.71B |      1.4x |     16.01s |   1.3x |    6.85 BIPS |  3.26 |       0.17% |
| rvr-rv64e      |    134.05B |    175.57B |      1.3x |     18.00s |   1.4x |    7.45 BIPS |  3.23 |       0.16% |
| rvr-rv64i      |    134.05B |    175.57B |      1.3x |     18.00s |   1.4x |    7.45 BIPS |  3.23 |       0.16% |
| libriscv-rv32e |          - |    422.26B |         - |     29.10s |   2.3x |            - |  4.80 |       0.09% |
| libriscv-rv32i |          - |    422.26B |         - |     29.11s |   2.3x |            - |  4.80 |       0.09% |
| libriscv-rv64e |          - |    585.58B |         - |     38.12s |   3.0x |            - |  5.08 |       0.11% |
| libriscv-rv64i |          - |    585.58B |         - |     38.12s |   3.0x |            - |  5.08 |       0.11% |

## minimal

*Minimal function call overhead | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv32i      |         30 |        438 |     14.6x |   166.67ns |      - |     180 MIPS |  1.27 |       4.00% |
| rvr-rv32e      |         30 |        429 |     14.3x |   236.33ns |      - |     127 MIPS |  0.41 |       4.00% |
| rvr-rv64i      |         30 |        440 |     14.7x |   236.33ns |      - |     127 MIPS |  0.43 |       4.00% |
| rvr-rv64e      |         30 |        430 |     14.3x |   250.33ns |      - |     120 MIPS |  0.42 |       4.00% |

## prime-sieve

*Prime number sieve algorithm | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |     17.59M |     20.00M |      1.1x |     2.02ms |      - |    8.72 BIPS |  3.28 |       0.20% |
| rvr-rv64i      |     16.18M |     19.99M |      1.2x |     2.14ms |      - |    7.57 BIPS |  3.10 |       0.20% |
| rvr-rv32e      |     23.79M |     26.26M |      1.1x |     2.34ms |      - |   10.17 BIPS |  3.71 |       0.18% |
| rvr-rv32i      |     21.36M |     28.87M |      1.4x |     2.62ms |      - |    8.16 BIPS |  3.64 |       0.18% |

## pinky

*NES emulator (cycle-accurate) | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |     30.81M |     44.00M |      1.4x |     3.30ms |      - |    9.34 BIPS |  4.41 |       1.72% |
| rvr-rv32e      |     31.90M |     44.69M |      1.4x |     3.39ms |      - |    9.42 BIPS |  4.36 |       1.68% |
| rvr-rv32i      |     31.12M |     45.21M |      1.5x |     3.45ms |      - |    9.02 BIPS |  4.35 |       1.67% |
| rvr-rv64i      |     30.88M |     46.11M |      1.5x |     3.55ms |      - |    8.70 BIPS |  4.32 |       1.73% |

## memset

*Memory set operations | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      1.57M |      3.15M |      2.0x |   225.45us |      - |    6.98 BIPS |  4.61 |       0.00% |
| rvr-rv64i      |      1.57M |      3.15M |      2.0x |   226.70us |      - |    6.94 BIPS |  4.57 |       0.00% |
| rvr-rv32e      |      3.15M |      6.29M |      2.0x |   452.22us |      - |    6.96 BIPS |  4.61 |       0.00% |
| rvr-rv32i      |      3.15M |      6.29M |      2.0x |   452.31us |      - |    6.95 BIPS |  4.61 |       0.00% |

## reth

*Reth block validator | runs: 3*

| Backend        |    Instret |   Host Ops | Ops/Guest |       Time |     OH |        Speed |   IPC | Branch Miss |
|----------------|------------|------------|-----------|------------|--------|--------------|-------|-------------|
| rvr-rv64e      |      3.36B |      3.49B |      1.0x |   239.65ms |      - |   14.02 BIPS |  4.92 |       1.23% |
| rvr-rv64i      |      2.88B |      3.39B |      1.2x |   241.63ms |      - |   11.94 BIPS |  4.68 |       1.20% |
| rvr-rv32i      |      7.27B |      9.36B |      1.3x |   578.17ms |      - |   12.58 BIPS |  5.36 |       0.76% |
| rvr-rv32e      |      8.55B |      9.81B |      1.1x |   648.09ms |      - |   13.19 BIPS |  5.08 |       0.81% |

