//! Benchmark registry - definitions and metadata for all benchmarks.

/// How to build a benchmark.
#[derive(Clone, Copy)]
pub enum BenchmarkSource {
    /// Rust project compiled to RISC-V ELF.
    Rust {
        path: &'static str,
        bin: &'static str,
    },
    /// Polkavm benchmark - build with benchmarks/build.sh
    Polkavm,
    /// C benchmark from riscv-tests - build with riscv-gcc
    RiscvTests,
    /// C benchmark from libriscv - build with riscv-gcc using riscv-tests runtime
    Libriscv,
    /// `CoreMark` benchmark from EEMBC
    Coremark,
}

/// Benchmark metadata.
pub struct BenchmarkInfo {
    /// Benchmark name (used in CLI and paths).
    pub name: &'static str,
    /// Whether benchmark uses `export_functions` mode (initialize/run pattern).
    pub uses_exports: bool,
    /// Default architectures for this benchmark.
    pub default_archs: &'static str,
    /// How to build this benchmark.
    pub source: BenchmarkSource,
}

/// All registered benchmarks.
/// ELF binaries are at: bin/{arch}/{name}
pub const BENCHMARKS: &[BenchmarkInfo] = &[
    // riscv-tests benchmarks (C-based)
    BenchmarkInfo {
        name: "towers",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "qsort",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "rsort",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "median",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "multiply",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "vvadd",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "memcpy",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "dhrystone",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::RiscvTests,
    },
    // libriscv benchmarks (use Linux syscalls, not HTIF)
    BenchmarkInfo {
        name: "fib",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::Libriscv,
    },
    BenchmarkInfo {
        name: "fib-asm",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::Libriscv,
    },
    // coremark benchmark
    BenchmarkInfo {
        name: "coremark",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::Coremark,
    },
    // polkavm benchmarks
    BenchmarkInfo {
        name: "minimal",
        uses_exports: true,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "prime-sieve",
        uses_exports: true,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "pinky",
        uses_exports: true,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "memset",
        uses_exports: true,
        default_archs: "rv64i",
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "reth",
        uses_exports: false,
        default_archs: "rv64i",
        source: BenchmarkSource::Rust {
            path: "programs/reth",
            bin: "reth",
        },
    },
];

/// Find benchmark by name.
pub fn find_benchmark(name: &str) -> Option<&'static BenchmarkInfo> {
    BENCHMARKS.iter().find(|b| b.name == name)
}
