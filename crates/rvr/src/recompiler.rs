use std::marker::PhantomData;
use std::path::Path;
use std::process::{Command, Stdio};

use rvr_elf::ElfImage;
use rvr_emit::{Backend, Compiler, EmitConfig, SyscallMode};
use rvr_isa::syscalls::{LinuxHandler, SyscallAbi};
use rvr_isa::{ExtensionRegistry, Xlen};
use tracing::{debug, error, info_span, warn};

use crate::{Error, Pipeline, Result};

/// RISC-V recompiler.
pub struct Recompiler<X: Xlen> {
    config: EmitConfig<X>,
    quiet: bool,
    export_functions: bool,
    _marker: PhantomData<X>,
}

impl<X: Xlen> Recompiler<X> {
    /// Create a new recompiler with the given configuration.
    pub fn new(config: EmitConfig<X>) -> Self {
        Self {
            config,
            quiet: false,
            export_functions: false,
            _marker: PhantomData,
        }
    }

    /// Create a recompiler with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(EmitConfig::default())
    }

    /// Set syscall handling mode.
    pub fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.config.syscall_mode = mode;
        self
    }

    /// Set the C compiler to use (clang or gcc).
    pub fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.config.compiler = compiler;
        self
    }

    /// Suppress compilation output.
    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Enable export functions mode for calling exported functions.
    ///
    /// When enabled, all function symbols are added as CFG entry points,
    /// and RV_EXPORT_FUNCTIONS metadata is set in the compiled library.
    pub fn with_export_functions(mut self, enabled: bool) -> Self {
        self.export_functions = enabled;
        self.config.export_functions = enabled;
        self
    }

    /// Get the configuration.
    pub fn config(&self) -> &EmitConfig<X> {
        &self.config
    }

    /// Compile an ELF file to a shared library.
    ///
    /// If `jobs` is 0, auto-detects based on CPU count.
    pub fn compile(
        &self,
        elf_path: &Path,
        output_dir: &Path,
        jobs: usize,
    ) -> Result<std::path::PathBuf> {
        let _span = info_span!(
            "compile",
            backend = ?self.config.backend,
            output = %output_dir.display()
        )
        .entered();
        // First lift to source (C or x86 assembly)
        let _source_path = self.lift(elf_path, output_dir)?;

        let lib_name = output_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rv");

        // Compile based on backend
        match self.config.backend {
            Backend::C => {
                // Compile C to .so (compiler choice is already in the Makefile via config)
                compile_c_to_shared(output_dir, jobs, self.quiet)?;
            }
            Backend::X86Asm => {
                // Assemble x86 to .so
                compile_x86_to_shared(output_dir, lib_name, &self.config.compiler, self.quiet)?;
            }
            Backend::ARM64Asm => {
                // Assemble ARM64 to .so
                compile_arm64_to_shared(output_dir, lib_name, &self.config.compiler, self.quiet)?;
            }
        }

        let lib_path = output_dir.join(format!("lib{}.so", lib_name));
        Ok(lib_path)
    }

    /// Lift an ELF file to source code (C or x86 assembly, depending on backend).
    pub fn lift(&self, elf_path: &Path, output_dir: &Path) -> Result<std::path::PathBuf> {
        let _span = info_span!(
            "lift",
            backend = ?self.config.backend,
            input = %elf_path.display(),
            output = %output_dir.display()
        )
        .entered();
        // Load ELF
        let data = {
            let _span = info_span!("load_elf").entered();
            std::fs::read(elf_path)?
        };
        let image = {
            let _span = info_span!("parse_elf").entered();
            ElfImage::<X>::parse(&data)?
        };

        // Create output directory if it doesn't exist
        std::fs::create_dir_all(output_dir)?;

        // Build pipeline with syscall handler selection.
        let registry = match self.config.syscall_mode {
            SyscallMode::BareMetal => ExtensionRegistry::standard(),
            SyscallMode::Linux => {
                let abi = if image.is_rve() {
                    SyscallAbi::Embedded
                } else {
                    SyscallAbi::Standard
                };
                ExtensionRegistry::standard().with_syscall_handler(LinuxHandler::new(abi))
            }
        };
        let mut pipeline = {
            let _span = info_span!("pipeline_init").entered();
            Pipeline::<X>::with_registry(image, self.config.clone(), registry)
        };

        // Add function symbols as extra entry points if requested
        if self.export_functions {
            pipeline.add_function_symbols_as_entry_points();
        }

        // Build CFG (InstructionTable → BlockTable → optimizations)
        pipeline.build_cfg()?;

        // Lift to IR
        // For C backend in per-instruction mode, use single-instruction blocks
        // to enable mid-block resume (dispatch table entry for every instruction PC)
        match self.config.backend {
            Backend::C if self.config.instret_mode.per_instruction() => {
                pipeline.lift_to_ir_as_single_blocks()?
            }
            Backend::C => pipeline.lift_to_ir()?,
            _ => pipeline.lift_to_ir_linear()?,
        }

        let base_name = output_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rv");

        // Emit based on backend
        match self.config.backend {
            Backend::C => {
                // Load debug info for #line directives (if enabled and ELF has debug info)
                if self.config.emit_line_info
                    && let Some(path_str) = elf_path.to_str()
                    && let Err(e) = pipeline.load_debug_info(path_str)
                {
                    warn!(error = %e, "failed to load debug info (continuing without #line directives)");
                }

                pipeline.emit_c(output_dir, base_name)?;
                Ok(output_dir.join(format!("{}_part0.c", base_name)))
            }
            Backend::X86Asm => {
                pipeline.emit_x86(output_dir, base_name)?;
                Ok(output_dir.join(format!("{}.s", base_name)))
            }
            Backend::ARM64Asm => {
                pipeline.emit_arm64(output_dir, base_name)?;
                Ok(output_dir.join(format!("{}.s", base_name)))
            }
        }
    }
}

/// Compile C source to shared library.
///
/// If `jobs` is 0, auto-detects based on CPU count.
/// Note: The compiler is set in the Makefile (generated with the chosen CC).
fn compile_c_to_shared(output_dir: &Path, jobs: usize, quiet: bool) -> Result<()> {
    let _span = info_span!("compile_c").entered();

    let makefile_path = output_dir.join("Makefile");
    if !makefile_path.exists() {
        error!(path = %makefile_path.display(), "Makefile not found");
        return Err(Error::CompilationFailed("Makefile not found".to_string()));
    }

    let job_count = if jobs == 0 {
        num_cpus::get().saturating_sub(2).max(1)
    } else {
        jobs
    };

    debug!(dir = %output_dir.display(), jobs = job_count, "running make");

    let mut cmd = Command::new("make");
    cmd.arg("-C")
        .arg(output_dir)
        .arg("-j")
        .arg(job_count.to_string())
        .arg("shared");

    // Always capture output so we can show errors on failure
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| {
        error!(error = %e, "failed to run make");
        Error::CompilationFailed(format!("Failed to run make: {}", e))
    })?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Log full output for debugging
        if !stderr.is_empty() {
            error!(exit_code = code, dir = %output_dir.display(), stderr = %stderr, "make failed");
        } else if !stdout.is_empty() {
            error!(exit_code = code, dir = %output_dir.display(), stdout = %stdout, "make failed");
        } else {
            error!(exit_code = code, dir = %output_dir.display(), "make failed");
        }
        // Include first line of error in the error message for quick visibility
        let first_error = stderr
            .lines()
            .next()
            .or_else(|| stdout.lines().next())
            .unwrap_or("unknown error");
        return Err(Error::CompilationFailed(format!(
            "make failed: {}",
            first_error
        )));
    } else if !quiet {
        // In non-quiet mode, show stdout (compilation progress)
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            for line in stdout.lines() {
                debug!("{}", line);
            }
        }
    }

    Ok(())
}

/// Compile x86 assembly to shared library.
///
/// On non-x86 hosts, uses clang for cross-compilation with:
/// - `--target=x86_64-unknown-linux-gnu` for x86 target
/// - `-fuse-ld=lld` for cross-linking
/// - `-nostdlib` since generated code is self-contained
fn compile_x86_to_shared(
    output_dir: &Path,
    base_name: &str,
    compiler: &Compiler,
    quiet: bool,
) -> Result<()> {
    let _span = info_span!("compile_x86").entered();

    let asm_path = output_dir.join(format!("{}.s", base_name));
    let obj_path = output_dir.join(format!("{}.o", base_name));
    let lib_path = output_dir.join(format!("lib{}.so", base_name));

    if !asm_path.exists() {
        return Err(Error::CompilationFailed(format!(
            "Assembly file not found: {}",
            asm_path.display()
        )));
    }

    // Check if we need cross-compilation (non-x86 host)
    let is_x86_host = cfg!(target_arch = "x86_64") || cfg!(target_arch = "x86");
    let needs_cross = !is_x86_host;

    let cc = if needs_cross {
        // On non-x86 hosts, must use clang for cross-compilation
        "clang"
    } else {
        compiler.command()
    };

    debug!(asm = %asm_path.display(), compiler = %cc, cross = %needs_cross, "assembling");

    // Assemble: cc -c -fPIC -o foo.o foo.s
    let mut asm_cmd = Command::new(cc);

    if needs_cross {
        // Cross-compilation: use clang with explicit x86 target
        asm_cmd.args(["--target=x86_64-unknown-linux-gnu", "-c", "-fPIC"]);
    } else {
        // AT&T syntax works with both GCC and LLVM's integrated assembler
        asm_cmd.args(["-c", "-fPIC"]);
    }

    asm_cmd.arg("-o").arg(&obj_path).arg(&asm_path);

    let asm_output = {
        let _span = info_span!("assemble").entered();
        asm_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to run {}: {}", cc, e)))?
    };

    if !asm_output.status.success() {
        let stderr = String::from_utf8_lossy(&asm_output.stderr);
        error!(stderr = %stderr, "assembly failed");
        return Err(Error::CompilationFailed(format!(
            "Assembly failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        )));
    }

    debug!(obj = %obj_path.display(), "linking");

    // Collect object files to link
    let mut obj_files = vec![obj_path.clone()];

    // Check for syscalls.c and compile it if present
    let syscalls_c_path = output_dir.join(format!("{}_syscalls.c", base_name));
    if syscalls_c_path.exists() {
        let syscalls_obj_path = output_dir.join(format!("{}_syscalls.o", base_name));
        let mut syscalls_cmd = Command::new(cc);

        if needs_cross {
            syscalls_cmd.args([
                "--target=x86_64-unknown-linux-gnu",
                "-c",
                "-fPIC",
                "-O2",
                "-std=c23",
            ]);
        } else {
            syscalls_cmd.args(["-c", "-fPIC", "-O2", "-std=c23"]);
        }

        syscalls_cmd
            .arg("-I")
            .arg(output_dir)
            .arg("-o")
            .arg(&syscalls_obj_path)
            .arg(&syscalls_c_path);

        let syscalls_output = {
            let _span = info_span!("compile_syscalls").entered();
            syscalls_cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| {
                    Error::CompilationFailed(format!("Failed to compile syscalls: {}", e))
                })?
        };

        if !syscalls_output.status.success() {
            let stderr = String::from_utf8_lossy(&syscalls_output.stderr);
            error!(stderr = %stderr, "syscalls compilation failed");
            return Err(Error::CompilationFailed(format!(
                "Syscalls compilation failed: {}",
                stderr.lines().next().unwrap_or("unknown error")
            )));
        }

        obj_files.push(syscalls_obj_path);
    }

    // Check for htif.c and compile it if present (for HTIF syscall support)
    let htif_c_path = output_dir.join(format!("{}_htif.c", base_name));
    if htif_c_path.exists() {
        let htif_obj_path = output_dir.join(format!("{}_htif.o", base_name));
        let mut htif_cmd = Command::new(cc);

        if needs_cross {
            htif_cmd.args([
                "--target=x86_64-unknown-linux-gnu",
                "-c",
                "-fPIC",
                "-O2",
                "-std=c23",
            ]);
        } else {
            htif_cmd.args(["-c", "-fPIC", "-O2", "-std=c23"]);
        }

        htif_cmd
            .arg("-I")
            .arg(output_dir)
            .arg("-o")
            .arg(&htif_obj_path)
            .arg(&htif_c_path);

        let htif_output = {
            let _span = info_span!("compile_htif").entered();
            htif_cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| Error::CompilationFailed(format!("Failed to compile htif: {}", e)))?
        };

        if !htif_output.status.success() {
            let stderr = String::from_utf8_lossy(&htif_output.stderr);
            error!(stderr = %stderr, "htif compilation failed");
            return Err(Error::CompilationFailed(format!(
                "HTIF compilation failed: {}",
                stderr.lines().next().unwrap_or("unknown error")
            )));
        }

        obj_files.push(htif_obj_path);
    }

    // Link to shared library
    let mut link_cmd = Command::new(cc);

    if needs_cross {
        // Cross-linking: use lld and no stdlib (our code is self-contained)
        link_cmd.args([
            "--target=x86_64-unknown-linux-gnu",
            "-fuse-ld=lld",
            "-nostdlib",
            "-shared",
            "-Wl,-z,noexecstack",
        ]);
    } else {
        link_cmd.args(["-shared", "-Wl,-z,noexecstack"]);
        // Use configured linker for clang (e.g., lld, lld-20)
        if let Some(linker) = compiler.linker() {
            link_cmd.arg(format!("-fuse-ld={}", linker));
        }
    }

    link_cmd.arg("-o").arg(&lib_path);
    for obj in &obj_files {
        link_cmd.arg(obj);
    }

    let link_output = {
        let _span = info_span!("link_shared").entered();
        link_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to link: {}", e)))?
    };

    if !link_output.status.success() {
        let stderr = String::from_utf8_lossy(&link_output.stderr);
        error!(stderr = %stderr, "linking failed");
        return Err(Error::CompilationFailed(format!(
            "Linking failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        )));
    }

    if !quiet {
        debug!(lib = %lib_path.display(), cross = %needs_cross, "compiled x86 shared library");
    }

    Ok(())
}

/// Compile ARM64 assembly to shared library.
///
/// On non-ARM64 hosts, uses clang for cross-compilation with:
/// - `--target=aarch64-unknown-linux-gnu` for ARM64 target
/// - `-fuse-ld=lld` for cross-linking
/// - `-nostdlib` since generated code is self-contained
fn compile_arm64_to_shared(
    output_dir: &Path,
    base_name: &str,
    compiler: &Compiler,
    quiet: bool,
) -> Result<()> {
    let _span = info_span!("compile_arm64").entered();

    let asm_path = output_dir.join(format!("{}.s", base_name));
    let obj_path = output_dir.join(format!("{}.o", base_name));
    let lib_path = output_dir.join(format!("lib{}.so", base_name));

    if !asm_path.exists() {
        return Err(Error::CompilationFailed(format!(
            "Assembly file not found: {}",
            asm_path.display()
        )));
    }

    // Check if we need cross-compilation (non-ARM64 host)
    let is_arm64_host = cfg!(target_arch = "aarch64");
    let needs_cross = !is_arm64_host;

    let cc = if needs_cross {
        // On non-ARM64 hosts, must use clang for cross-compilation
        "clang"
    } else {
        compiler.command()
    };

    debug!(asm = %asm_path.display(), compiler = %cc, cross = %needs_cross, "assembling");

    // Assemble: cc -c -fPIC -o foo.o foo.s
    let mut asm_cmd = Command::new(cc);

    if needs_cross {
        // Cross-compilation: use clang with explicit ARM64 target
        asm_cmd.args(["--target=aarch64-unknown-linux-gnu", "-c", "-fPIC"]);
    } else {
        asm_cmd.args(["-c", "-fPIC"]);
    }

    asm_cmd.arg("-o").arg(&obj_path).arg(&asm_path);

    let asm_output = {
        let _span = info_span!("assemble").entered();
        asm_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to run {}: {}", cc, e)))?
    };

    if !asm_output.status.success() {
        let stderr = String::from_utf8_lossy(&asm_output.stderr);
        error!(stderr = %stderr, "assembly failed");
        return Err(Error::CompilationFailed(format!(
            "Assembly failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        )));
    }

    debug!(obj = %obj_path.display(), "linking");

    // Collect object files to link
    let mut obj_files = vec![obj_path.clone()];

    // Check for syscalls.c and compile it if present
    let syscalls_c_path = output_dir.join(format!("{}_syscalls.c", base_name));
    if syscalls_c_path.exists() {
        let syscalls_obj_path = output_dir.join(format!("{}_syscalls.o", base_name));
        let mut syscalls_cmd = Command::new(cc);

        if needs_cross {
            syscalls_cmd.args([
                "--target=aarch64-unknown-linux-gnu",
                "-c",
                "-fPIC",
                "-O2",
                "-std=c23",
            ]);
        } else {
            syscalls_cmd.args(["-c", "-fPIC", "-O2", "-std=c23"]);
        }

        syscalls_cmd
            .arg("-I")
            .arg(output_dir)
            .arg("-o")
            .arg(&syscalls_obj_path)
            .arg(&syscalls_c_path);

        let syscalls_output = syscalls_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to compile syscalls: {}", e)))?;

        if !syscalls_output.status.success() {
            let stderr = String::from_utf8_lossy(&syscalls_output.stderr);
            error!(stderr = %stderr, "syscalls compilation failed");
            return Err(Error::CompilationFailed(format!(
                "Syscalls compilation failed: {}",
                stderr.lines().next().unwrap_or("unknown error")
            )));
        }

        obj_files.push(syscalls_obj_path);
    }

    // Check for htif.c and compile it if present (for HTIF syscall support)
    let htif_c_path = output_dir.join(format!("{}_htif.c", base_name));
    if htif_c_path.exists() {
        let htif_obj_path = output_dir.join(format!("{}_htif.o", base_name));
        let mut htif_cmd = Command::new(cc);

        if needs_cross {
            htif_cmd.args([
                "--target=aarch64-unknown-linux-gnu",
                "-c",
                "-fPIC",
                "-O2",
                "-std=c23",
            ]);
        } else {
            htif_cmd.args(["-c", "-fPIC", "-O2", "-std=c23"]);
        }

        htif_cmd
            .arg("-I")
            .arg(output_dir)
            .arg("-o")
            .arg(&htif_obj_path)
            .arg(&htif_c_path);

        let htif_output = htif_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to compile htif: {}", e)))?;

        if !htif_output.status.success() {
            let stderr = String::from_utf8_lossy(&htif_output.stderr);
            error!(stderr = %stderr, "htif compilation failed");
            return Err(Error::CompilationFailed(format!(
                "HTIF compilation failed: {}",
                stderr.lines().next().unwrap_or("unknown error")
            )));
        }

        obj_files.push(htif_obj_path);
    }

    // Link to shared library
    let mut link_cmd = Command::new(cc);

    if needs_cross {
        // Cross-linking: use lld and no stdlib (our code is self-contained)
        link_cmd.args([
            "--target=aarch64-unknown-linux-gnu",
            "-fuse-ld=lld",
            "-nostdlib",
            "-shared",
            "-Wl,-z,noexecstack",
        ]);
    } else {
        link_cmd.args(["-shared", "-Wl,-z,noexecstack"]);
        // Use configured linker for clang (e.g., lld, lld-20)
        if let Some(linker) = compiler.linker() {
            link_cmd.arg(format!("-fuse-ld={}", linker));
        }
    }

    link_cmd.arg("-o").arg(&lib_path);
    for obj in &obj_files {
        link_cmd.arg(obj);
    }

    let link_output = link_cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::CompilationFailed(format!("Failed to link: {}", e)))?;

    if !link_output.status.success() {
        let stderr = String::from_utf8_lossy(&link_output.stderr);
        error!(stderr = %stderr, "linking failed");
        return Err(Error::CompilationFailed(format!(
            "Linking failed: {}",
            stderr.lines().next().unwrap_or("unknown error")
        )));
    }

    if !quiet {
        debug!(lib = %lib_path.display(), cross = %needs_cross, "compiled ARM64 shared library");
    }

    Ok(())
}
