//! Generate BENCHMARKS.md using cargo bench helpers.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use rvr::bench::{self, Arch};
use rvr::{AddressMode, CompileOptions, Compiler, InstretMode, SyscallMode};
use rvr_emit::Backend;

#[path = "../../benches/support/mod.rs"]
mod bench_support;

use bench_support::{
    coremark, find_project_root, libriscv, polkavm, riscv_tests, rust_bench, BenchmarkInfo,
    BenchmarkSource,
};

#[derive(Parser, Debug)]
#[command(name = "bench_report")]
#[command(about = "Generate BENCHMARKS.md from cargo bench helpers")]
struct Args {
    /// Backend to use for compilation
    #[arg(long, value_parser = ["c", "x86", "arm64"], default_value = "c")]
    backend: String,

    /// Number of runs per benchmark
    #[arg(long, default_value_t = 1)]
    runs: usize,

    /// Substring filter for benchmark names
    #[arg(long)]
    filter: Option<String>,

    /// Output path for BENCHMARKS.md
    #[arg(long, default_value = "BENCHMARKS.md")]
    output: PathBuf,

    /// Rebuild RISC-V ELFs before compiling
    #[arg(long)]
    rebuild_elfs: bool,

    /// Recompile native shared libs before running
    #[arg(long)]
    recompile: bool,

    /// Enable perf mode (disable instret, enable perf)
    #[arg(long)]
    perf: bool,
}

fn parse_backend(arg: &str) -> Backend {
    match arg {
        "x86" => Backend::X86Asm,
        "arm64" => Backend::ARM64Asm,
        _ => Backend::C,
    }
}

fn filter_match(name: &str, filter: &Option<String>) -> bool {
    match filter {
        Some(f) if !f.trim().is_empty() => name.contains(f),
        _ => true,
    }
}

fn should_rebuild(args: &Args) -> bool {
    args.rebuild_elfs
}

fn should_recompile(args: &Args) -> bool {
    args.recompile
}

fn bench_output_dir(
    project_dir: &PathBuf,
    info: &BenchmarkInfo,
    arch: Arch,
    backend: Backend,
) -> PathBuf {
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

fn compile_options(info: &BenchmarkInfo, backend: Backend, args: &Args) -> CompileOptions {
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

    if args.perf {
        options = options.with_perf_mode(true).with_instret_mode(InstretMode::Off);
    }

    options
}

fn ensure_elf(
    project_dir: &PathBuf,
    info: &BenchmarkInfo,
    arch: Arch,
    args: &Args,
) -> Result<PathBuf, String> {
    let out_dir = project_dir.join("bin").join(arch.as_str());
    let elf_path = out_dir.join(info.name);
    if elf_path.exists() && !should_rebuild(args) {
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
    args: &Args,
) -> Result<PathBuf, String> {
    let out_dir = bench_output_dir(project_dir, info, arch, backend);
    let so_path = out_dir.join(format!(
        "lib{}.{}",
        info.name,
        if cfg!(target_os = "macos") { "dylib" } else { "so" }
    ));
    if so_path.exists() && !should_recompile(args) {
        return Ok(out_dir);
    }

    let elf_path = ensure_elf(project_dir, info, arch, args)?;
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create output dir: {}", e))?;
    let options = compile_options(info, backend, args);
    rvr::compile_with_options(&elf_path, &out_dir, options)
        .map_err(|e| format!("compile failed: {}", e))?;
    Ok(out_dir)
}

fn collect_system_info() -> Vec<(String, String)> {
    let mut info = Vec::new();

    if let Ok(output) = Command::new("uname").arg("-m").output()
        && output.status.success()
    {
        let arch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info.push(("Architecture".to_string(), arch));
    }

    if let Ok(output) = Command::new("rustc").arg("--version").output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info.push(("Rust".to_string(), version));
    }

    if let Ok(output) = Command::new("clang").arg("--version").output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        info.push(("Clang".to_string(), version));
    }

    if let Ok(contents) = fs::read_to_string("/etc/os-release") {
        for line in contents.lines() {
            if line.starts_with("PRETTY_NAME=") {
                let name = line
                    .trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string();
                info.push(("OS".to_string(), name));
                break;
            }
        }
    }

    if let Ok(output) = Command::new("date").arg("+%Y-%m-%d %H:%M:%S").output()
        && output.status.success()
    {
        let date = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info.push(("Date".to_string(), date));
    }

    info
}

fn render_markdown(rows: &[(String, String, String, f64, f64)]) -> String {
    let mut out = String::new();
    out.push_str("# Benchmarks\n\n");

    out.push_str("## System Info\n\n");
    for (k, v) in collect_system_info() {
        out.push_str(&format!("- {}: {}\n", k, v));
    }
    out.push('\n');

    out.push_str("## Results\n\n");
    out.push_str("| Benchmark | Arch | Backend | Time (s) | MIPS |\n");
    out.push_str("|---|---|---|---:|---:|\n");
    for (name, arch, backend, time, mips) in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {:.6} | {:.2} |\n",
            name, arch, backend, time, mips
        ));
    }

    out
}

fn main() {
    let args = Args::parse();
    let project_dir = find_project_root();
    let backend = parse_backend(&args.backend);
    let runs = args.runs.max(1);
    if let Some(filter) = &args.filter {
        if !filter.contains('*') {
            let _ = bench_support::registry::find_benchmark(filter);
        }
    }

    let mut rows = Vec::new();

    for info in bench_support::registry::BENCHMARKS {
        if !filter_match(info.name, &args.filter) {
            continue;
        }
        let archs = Arch::parse_list(info.default_archs).unwrap_or_else(|_| vec![Arch::Rv64i]);
        for arch in archs {
            let out_dir = match ensure_compiled(&project_dir, info, arch, backend, &args) {
                Ok(dir) => dir,
                Err(err) => {
                    eprintln!("{} ({}) skipped: {}", info.name, arch.as_str(), err);
                    continue;
                }
            };
            let elf_path = project_dir
                .join("bin")
                .join(arch.as_str())
                .join(info.name);
            let result = match bench::run_bench_auto(&out_dir, &elf_path, runs) {
                Ok((result, _)) => result,
                Err(err) => {
                    eprintln!("{} ({}) failed: {}", info.name, arch.as_str(), err);
                    continue;
                }
            };

            let backend_label = match backend {
                Backend::C => "c",
                Backend::X86Asm => "x86",
                Backend::ARM64Asm => "arm64",
            };

            rows.push((
                info.name.to_string(),
                arch.as_str().to_string(),
                backend_label.to_string(),
                result.result.time_secs,
                result.result.mips,
            ));
        }
    }

    let output = render_markdown(&rows);
    let out_path = args.output;
    if let Err(err) = fs::write(&out_path, output) {
        eprintln!("failed to write {}: {}", out_path.display(), err);
        std::process::exit(1);
    }
    println!("wrote {}", out_path.display());
}
