#![feature(test)]

extern crate test;

use std::path::PathBuf;

use test::Bencher;

use rvr::bench::{self, Arch};
#[path = "support/mod.rs"]
mod bench_support;

use bench_support::{
    coremark, find_project_root, libriscv, polkavm, riscv_tests, rust_bench, BenchmarkInfo,
    BenchmarkSource,
};
use bench_support::registry::find_benchmark;
use rvr::{AddressMode, CompileOptions, Compiler, InstretMode, SyscallMode};
use rvr_emit::Backend;

fn parse_backend() -> Backend {
    match std::env::var("RVR_BENCH_BACKEND").as_deref() {
        Ok("x86") | Ok("x86_64") => Backend::X86Asm,
        Ok("arm64") | Ok("aarch64") => Backend::ARM64Asm,
        _ => Backend::C,
    }
}

fn parse_arch(info: &BenchmarkInfo) -> Arch {
    let archs = Arch::parse_list(info.default_archs)
        .unwrap_or_else(|_| vec![Arch::Rv64i]);
    archs[0]
}

fn should_rebuild() -> bool {
    std::env::var("RVR_REBUILD_ELFS").is_ok()
}

fn should_recompile() -> bool {
    std::env::var("RVR_RECOMPILE_BENCH").is_ok()
}

fn bench_output_dir(project_dir: &PathBuf, info: &BenchmarkInfo, arch: Arch, backend: Backend) -> PathBuf {
    let suffix = match backend {
        Backend::C => "base",
        Backend::X86Asm => "x86",
        Backend::ARM64Asm => "arm64",
    };
    project_dir
        .join("target/benchmarks")
        .join(info.name)
        .join(arch.as_str())
        .join(suffix)
}

fn compile_options(info: &BenchmarkInfo, backend: Backend) -> CompileOptions {
    let mut options = CompileOptions::new()
        .with_compiler(Compiler::default())
        .with_backend(backend)
        .with_export_functions(info.uses_exports)
        .with_address_mode(AddressMode::Wrap)
        .with_quiet(true);

    match info.source {
        BenchmarkSource::RiscvTests => {
            options = options.with_htif(true);
        }
        BenchmarkSource::Libriscv | BenchmarkSource::Coremark => {
            options = options.with_syscall_mode(SyscallMode::Linux);
        }
        _ => {}
    }

    if matches!(std::env::var("RVR_BENCH_PERF"), Ok(_)) {
        options = options.with_perf_mode(true).with_instret_mode(InstretMode::Off);
    }

    options
}

fn ensure_elf(project_dir: &PathBuf, info: &BenchmarkInfo, arch: Arch) -> Result<PathBuf, String> {
    let out_dir = project_dir.join("bin").join(arch.as_str());
    let elf_path = out_dir.join(info.name);
    if elf_path.exists() && !should_rebuild() {
        return Ok(elf_path);
    }

    match info.source {
        BenchmarkSource::RiscvTests => riscv_tests::build_benchmark(project_dir, info.name, &arch),
        BenchmarkSource::Libriscv => libriscv::build_benchmark(project_dir, info.name, &arch),
        BenchmarkSource::Coremark => coremark::build_benchmark(project_dir, &arch),
        BenchmarkSource::Polkavm => polkavm::build_benchmark(project_dir, info.name, arch.as_str()),
        BenchmarkSource::Rust { path, bin } => {
            rust_bench::build_benchmark(project_dir, path, arch, Some(bin))
        }
    }
}

fn ensure_compiled(
    project_dir: &PathBuf,
    info: &BenchmarkInfo,
    arch: Arch,
    backend: Backend,
) -> Result<PathBuf, String> {
    let out_dir = bench_output_dir(project_dir, info, arch, backend);
    let so_path = out_dir.join(format!("lib{}.{}", info.name, if cfg!(target_os = "macos") { "dylib" } else { "so" }));
    if so_path.exists() && !should_recompile() {
        return Ok(out_dir);
    }

    let elf_path = ensure_elf(project_dir, info, arch)?;
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create output dir: {}", e))?;
    let options = compile_options(info, backend);
    rvr::compile_with_options(&elf_path, &out_dir, options)
        .map_err(|e| format!("compile failed: {}", e))?;
    Ok(out_dir)
}

fn run_once(info: &BenchmarkInfo) -> Result<(), String> {
    let project_dir = find_project_root();
    let backend = parse_backend();
    let arch = parse_arch(info);
    let out_dir = ensure_compiled(&project_dir, info, arch, backend)?;
    let elf_path = project_dir.join("bin").join(arch.as_str()).join(info.name);
    let _ = bench::run_bench_auto(&out_dir, &elf_path, 1)?;
    Ok(())
}

fn bench_case(name: &str, b: &mut Bencher) {
    let info = find_benchmark(name).unwrap_or_else(|| panic!("unknown benchmark: {}", name));
    let _ = run_once(info);
    b.iter(|| {
        let _ = run_once(info);
    });
}

macro_rules! bench_entry {
    ($fn_name:ident, $name:expr) => {
        #[bench]
        fn $fn_name(b: &mut Bencher) {
            bench_case($name, b);
        }
    };
}

bench_entry!(bench_towers, "towers");
bench_entry!(bench_qsort, "qsort");
bench_entry!(bench_rsort, "rsort");
bench_entry!(bench_median, "median");
bench_entry!(bench_multiply, "multiply");
bench_entry!(bench_vvadd, "vvadd");
bench_entry!(bench_memcpy, "memcpy");
bench_entry!(bench_dhrystone, "dhrystone");
bench_entry!(bench_fib, "fib");
bench_entry!(bench_fib_asm, "fib-asm");
bench_entry!(bench_coremark, "coremark");
bench_entry!(bench_minimal, "minimal");
bench_entry!(bench_prime_sieve, "prime-sieve");
bench_entry!(bench_pinky, "pinky");
bench_entry!(bench_reth, "reth");
