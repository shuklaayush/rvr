use std::path::{Path, PathBuf};

use libtest_mimic::{Arguments, Failed, Trial};
use rvr::test_support::diff;
use rvr::{Compiler, Runner};
use rvr_emit::Backend;
use rvr_elf::{ElfImage, get_elf_xlen};
use rvr_ir::{Rv32, Rv64};

mod test_utils;

fn main() {
    let mut args = Arguments::from_args();
    test_utils::cap_threads(&mut args);

    let trials = vec![
        Trial::test("diff_lockstep_c", || run_lockstep()),
        Trial::test("diff_block_vs_linear_c", || run_block_vs_linear()),
        Trial::test("diff_checkpoint_c", || run_checkpoint()),
        Trial::test("diff_pure_c", || run_pure_c()),
    ];

    libtest_mimic::run(&args, trials).exit();
}

fn run_lockstep() -> Result<(), Failed> {
    let elf_path = match diff_elf_path() {
        Some(path) => path,
        None => return Ok(()),
    };

    let temp = tempfile::tempdir().map_err(|e| Failed::from(format!("tempdir: {e}")))?;
    let ref_dir = temp.path().join("ref");
    let test_dir = temp.path().join("test");

    let compiler = Compiler::default();
    diff::compile_for_diff(&elf_path, &ref_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;
    diff::compile_for_diff(&elf_path, &test_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;

    let mut ref_exec = diff::InProcessExecutor::new(&ref_dir, &elf_path)
        .map_err(|e| Failed::from(format!("ref load: {e}")))?;
    let mut test_exec = diff::InProcessExecutor::new(&test_dir, &elf_path)
        .map_err(|e| Failed::from(format!("test load: {e}")))?;

    let config = diff::CompareConfig {
        strict_reg_writes: true,
        strict_mem_access: true,
    };

    let result = diff::compare_lockstep(&mut ref_exec, &mut test_exec, &config, Some(200));
    if let Some(div) = result.divergence {
        return Err(Failed::from(format!("divergence: {:?}", div.kind)));
    }

    Ok(())
}

fn run_block_vs_linear() -> Result<(), Failed> {
    let elf_path = match diff_elf_path() {
        Some(path) => path,
        None => return Ok(()),
    };

    let temp = tempfile::tempdir().map_err(|e| Failed::from(format!("tempdir: {e}")))?;
    let block_dir = temp.path().join("block");
    let linear_dir = temp.path().join("linear");

    let compiler = Compiler::default();
    diff::compile_for_diff_block(&elf_path, &block_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;
    diff::compile_for_diff(&elf_path, &linear_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;

    let mut block_exec = diff::BufferedInProcessExecutor::new(&block_dir, &elf_path)
        .map_err(|e| Failed::from(format!("block load: {e}")))?;
    let mut linear_exec = diff::InProcessExecutor::new(&linear_dir, &elf_path)
        .map_err(|e| Failed::from(format!("linear load: {e}")))?;

    let config = diff::CompareConfig {
        strict_reg_writes: true,
        strict_mem_access: true,
    };

    let result = diff::compare_block_vs_linear(&mut block_exec, &mut linear_exec, &config, Some(200));
    if let Some(div) = result.divergence {
        return Err(Failed::from(format!("divergence: {:?}", div.kind)));
    }

    Ok(())
}

fn run_checkpoint() -> Result<(), Failed> {
    let elf_path = match diff_elf_path() {
        Some(path) => path,
        None => return Ok(()),
    };

    let temp = tempfile::tempdir().map_err(|e| Failed::from(format!("tempdir: {e}")))?;
    let ref_dir = temp.path().join("ref");
    let test_dir = temp.path().join("test");

    let compiler = Compiler::default();
    diff::compile_for_checkpoint(&elf_path, &ref_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;
    diff::compile_for_checkpoint(&elf_path, &test_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;

    let mut ref_runner = Runner::load(&ref_dir, &elf_path)
        .map_err(|e| Failed::from(format!("ref load: {e}")))?;
    let mut test_runner = Runner::load(&test_dir, &elf_path)
        .map_err(|e| Failed::from(format!("test load: {e}")))?;

    ref_runner.prepare();
    test_runner.prepare();

    let entry = ref_runner.entry_point();
    ref_runner.set_pc(entry);
    test_runner.set_pc(entry);

    let result = diff::compare_checkpoint(&mut ref_runner, &mut test_runner, 128, Some(500));
    if let Some(div) = result.divergence {
        return Err(Failed::from(format!("divergence: {:?}", div.kind)));
    }

    Ok(())
}

fn run_pure_c() -> Result<(), Failed> {
    let elf_path = match diff_elf_path() {
        Some(path) => path,
        None => return Ok(()),
    };

    let temp = tempfile::tempdir().map_err(|e| Failed::from(format!("tempdir: {e}")))?;
    let ref_dir = temp.path().join("ref");
    let test_dir = temp.path().join("test");

    let compiler = Compiler::default();
    diff::compile_for_checkpoint(&elf_path, &ref_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;
    diff::compile_for_checkpoint(&elf_path, &test_dir, Backend::C, &compiler)
        .map_err(Failed::from)?;

    let data = std::fs::read(&elf_path)
        .map_err(|e| Failed::from(format!("read elf: {e}")))?;
    let xlen = get_elf_xlen(&data).map_err(Failed::from)?;

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());

    match xlen {
        32 => run_pure_c_for::<Rv32>(&data, temp.path(), &ref_dir, &test_dir, &cc),
        64 => run_pure_c_for::<Rv64>(&data, temp.path(), &ref_dir, &test_dir, &cc),
        _ => Err(Failed::from("unsupported XLEN")),
    }
}

fn run_pure_c_for<X: rvr_ir::Xlen>(
    data: &[u8],
    out_dir: &Path,
    ref_dir: &Path,
    test_dir: &Path,
    cc: &str,
) -> Result<(), Failed> {
    let image = ElfImage::<X>::parse(data).map_err(Failed::from)?;

    let default_mem_bits = rvr_emit::EmitConfig::<X>::default().memory_bits;
    let initial_sp = image.lookup_symbol("__stack_top").unwrap_or(0);
    let initial_gp = image.lookup_symbol("__global_pointer$").unwrap_or(0);

    let config = diff::CCompareConfig {
        entry_point: X::to_u64(image.entry_point),
        max_instrs: 500,
        checkpoint_interval: 128,
        memory_bits: default_mem_bits,
        num_regs: 32,
        instret_suspend: true,
        initial_brk: X::to_u64(image.get_initial_program_break()),
        initial_sp,
        initial_gp,
    };

    diff::generate_c_compare::<X>(out_dir, &image.memory_segments, &config)
        .map_err(|e| Failed::from(format!("generate c compare: {e}")))?;

    if !diff::compile_c_compare(out_dir, cc) {
        return Err(Failed::from("compile c compare failed"));
    }

    let ref_lib = find_library_in_dir(ref_dir)
        .ok_or_else(|| Failed::from("missing ref library"))?;
    let test_lib = find_library_in_dir(test_dir)
        .ok_or_else(|| Failed::from("missing test library"))?;

    let output = diff::run_c_compare(out_dir, &ref_lib, &test_lib)
        .map_err(|e| Failed::from(format!("run c compare: {e}")))?;
    if !output.status.success() {
        return Err(Failed::from("pure-c comparison failed"));
    }

    Ok(())
}

fn diff_elf_path() -> Option<PathBuf> {
    let root = workspace_root();
    let candidates = [
        "bin/rv64i/minimal",
        "bin/rv32i/minimal",
        "bin/riscv-tests/rv64ui-p-add",
        "bin/riscv-tests/rv32ui-p-add",
    ];
    for rel in candidates {
        let path = root.join(rel);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn find_library_in_dir(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.starts_with("lib")
            && name.ends_with(".so")
        {
            return Some(path);
        }
    }
    None
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("missing workspace root")
        .to_path_buf()
}
