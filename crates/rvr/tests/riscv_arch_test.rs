use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use libtest_mimic::{Arguments, Failed, Trial};
use rvr_emit::Backend;

#[path = "support/riscv_arch_test.rs"]
mod support;
mod test_utils;

fn main() {
    let mut args = Arguments::from_args();
    test_utils::cap_threads(&mut args);

    if std::env::var("RVR_BUILD_ONLY").is_ok() {
        if let Err(err) = build_only() {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }

    let cases = collect_arch_tests();
    let backends = enabled_backends();

    let mut trials = Vec::new();
    for backend in backends {
        let backend_name = backend_label(backend);
        for (elf, reference) in &cases {
            let name = format!("{}::{}", backend_name, ident_from_path(elf));
            let elf = elf.clone();
            let reference = reference.clone();
            trials.push(Trial::test(name, move || {
                run_case(&elf, &reference, backend)
            }));
        }
    }

    libtest_mimic::run(&args, trials).exit();
}

fn run_case(elf: &Path, reference: &Path, backend: Backend) -> Result<(), Failed> {
    let _ = maybe_rebuild_elfs();
    let timeout = Duration::from_secs(10);
    let compiler = rvr::Compiler::default();
    let root = workspace_root();
    let elf_path = root.join(elf);
    let ref_path = root.join(reference);
    if !ref_path.exists() || !elf_path.exists() {
        return Ok(());
    }
    let result = support::run_test(
        elf_path.as_path(),
        ref_path.as_path(),
        timeout,
        &compiler,
        backend,
    );
    match result {
        Ok(()) => Ok(()),
        Err(err) => Err(Failed::from(err)),
    }
}

fn enabled_backends() -> Vec<Backend> {
    let mut backends = vec![Backend::C];
    #[cfg(target_arch = "aarch64")]
    {
        backends.push(Backend::ARM64Asm);
    }
    #[cfg(target_arch = "x86_64")]
    {
        backends.push(Backend::X86Asm);
    }
    backends
}

const fn backend_label(backend: Backend) -> &'static str {
    match backend {
        Backend::C => "backend_c",
        Backend::ARM64Asm => "backend_arm64",
        Backend::X86Asm => "backend_x86",
    }
}

fn maybe_rebuild_elfs() -> Result<(), Failed> {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    let mut status = Ok(());
    ONCE.call_once(|| {
        let root = workspace_root();
        let bins = root.join("bin/riscv-arch-test");
        if bins.exists() {
            return;
        }
        let toolchain = support::find_toolchain().unwrap_or_default();
        if toolchain.is_empty() {
            return;
        }
        let config = support::ArchBuildConfig::new(support::ArchTestCategory::ALL.to_vec())
            .with_src_dir(root.join("programs/riscv-arch-test/riscv-test-suite"))
            .with_out_dir(root.join("bin/riscv-arch-test"))
            .with_refs_dir(root.join("bin/riscv-arch-test/references"))
            .with_toolchain(toolchain)
            .with_gen_refs(true);

        if let Err(err) = support::build_tests(&config) {
            status = Err(Failed::from(format!("failed to build arch tests: {err}")));
        }
    });
    status
}

fn build_only() -> Result<(), String> {
    let root = workspace_root();
    let toolchain = support::find_toolchain().unwrap_or_default();
    if toolchain.is_empty() {
        return Err("RISC-V toolchain not found".to_string());
    }
    let gen_refs = std::env::var("RVR_GEN_REFS").is_ok();
    let config = support::ArchBuildConfig::new(support::ArchTestCategory::ALL.to_vec())
        .with_src_dir(root.join("programs/riscv-arch-test/riscv-test-suite"))
        .with_out_dir(root.join("bin/riscv-arch-test"))
        .with_refs_dir(root.join("bin/riscv-arch-test/references"))
        .with_toolchain(toolchain)
        .with_gen_refs(gen_refs);
    support::build_tests(&config).map_err(|err| format!("failed to build arch tests: {err}"))
}

fn collect_arch_tests() -> Vec<(PathBuf, PathBuf)> {
    let root = workspace_root();
    let dir = root.join("bin/riscv-arch-test");
    let mut cases = Vec::new();
    if dir.exists() {
        let _ = collect_arch_cases(&dir, &dir, &mut cases);
    }
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    cases
}

fn collect_arch_cases(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(PathBuf, PathBuf)>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("references") {
                continue;
            }
            collect_arch_cases(root, &path, out)?;
        } else if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("sig") {
                continue;
            }
            let category = path
                .parent()
                .and_then(|p| p.file_name())
                .unwrap_or_default();
            let ref_path = root.join("references").join(category).join(format!(
                "{}.sig",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("")
            ));
            out.push((path, ref_path));
        }
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("missing workspace root")
        .to_path_buf()
}

fn ident_from_path(path: &Path) -> String {
    let root = workspace_root();
    let rel = path.strip_prefix(&root).unwrap_or(path);
    let mut s = String::new();
    for ch in rel.to_string_lossy().chars() {
        if ch.is_ascii_alphanumeric() {
            s.push(ch);
        } else {
            s.push('_');
        }
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}
