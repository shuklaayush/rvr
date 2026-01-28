//! Benchmark registry - definitions and metadata for all benchmarks.

use rvr::bench::Arch;

/// How to build a benchmark.
#[derive(Clone, Copy)]
pub enum BenchmarkSource {
    /// Rust project - build with `rvr build`
    Rust {
        /// Path to project directory (relative to repo root)
        path: &'static str,
    },
    /// Polkavm benchmark - build with benchmarks/build.sh
    Polkavm,
    /// C benchmark from riscv-tests - build with riscv-gcc
    RiscvTests,
    /// C benchmark from libriscv - build with riscv-gcc using riscv-tests runtime
    Libriscv,
    /// CoreMark benchmark from EEMBC
    Coremark,
}

/// Benchmark metadata.
pub struct BenchmarkInfo {
    /// Benchmark name (used in CLI and paths).
    pub name: &'static str,
    /// Short description.
    pub description: &'static str,
    /// Whether benchmark uses export_functions mode (initialize/run pattern).
    pub uses_exports: bool,
    /// Path to host binary relative to project root (for comparison).
    pub host_binary: Option<&'static str>,
    /// Default architectures for this benchmark.
    pub default_archs: &'static str,
    /// Supported architectures (None = all, Some = only those listed).
    pub supported_archs: Option<&'static str>,
    /// Whether a host version can be built.
    pub has_host: bool,
    /// How to build this benchmark.
    pub source: BenchmarkSource,
}

impl BenchmarkInfo {
    /// Check if this benchmark supports the given architecture.
    pub fn supports_arch(&self, arch: &Arch) -> bool {
        match self.supported_archs {
            None => true,
            Some(archs) => Arch::parse_list(archs)
                .map(|list| list.contains(arch))
                .unwrap_or(false),
        }
    }
}

/// All registered benchmarks.
/// ELF binaries are at: bin/{arch}/{name}
pub const BENCHMARKS: &[BenchmarkInfo] = &[
    // riscv-tests benchmarks (C-based)
    BenchmarkInfo {
        name: "towers",
        description: "Towers of Hanoi (recursive)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "qsort",
        description: "Quick sort algorithm",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "rsort",
        description: "Radix sort algorithm",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "median",
        description: "Median filter",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "multiply",
        description: "Software multiply",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "vvadd",
        description: "Vector-vector addition",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "memcpy",
        description: "Memory copy operations",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    BenchmarkInfo {
        name: "dhrystone",
        description: "Classic Dhrystone benchmark",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::RiscvTests,
    },
    // libriscv benchmarks (use Linux syscalls, not HTIF)
    BenchmarkInfo {
        name: "fib",
        description: "Fibonacci (recursive tail-call)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Libriscv,
    },
    BenchmarkInfo {
        name: "fib-asm",
        description: "Fibonacci (hand-written assembly)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: Some("rv64i,rv64e"), // RV64 assembly only
        has_host: false,                      // Can't compile assembly for host
        source: BenchmarkSource::Libriscv,
    },
    // coremark benchmark
    BenchmarkInfo {
        name: "coremark",
        description: "CoreMark CPU benchmark (EEMBC)",
        uses_exports: false,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Coremark,
    },
    // polkavm benchmarks
    BenchmarkInfo {
        name: "minimal",
        description: "Minimal function call overhead",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "prime-sieve",
        description: "Prime number sieve algorithm",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "pinky",
        description: "NES emulator (cycle-accurate)",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    BenchmarkInfo {
        name: "memset",
        description: "Memory set operations",
        uses_exports: true,
        host_binary: None,
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Polkavm,
    },
    // rust benchmarks
    BenchmarkInfo {
        name: "reth",
        description: "Reth block validator",
        uses_exports: false,
        host_binary: Some("programs/reth/target/release/reth"),
        default_archs: "rv64i",
        supported_archs: None,
        has_host: true,
        source: BenchmarkSource::Rust {
            path: "programs/reth",
        },
    },
];

/// Find benchmark by name.
pub fn find_benchmark(name: &str) -> Option<&'static BenchmarkInfo> {
    BENCHMARKS.iter().find(|b| b.name == name)
}
