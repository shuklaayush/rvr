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
    #[must_use]
    pub const fn new(config: EmitConfig<X>) -> Self {
        Self {
            config,
            quiet: false,
            export_functions: false,
            _marker: PhantomData,
        }
    }

    /// Create a recompiler with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(EmitConfig::default())
    }

    /// Set syscall handling mode.
    #[must_use]
    pub const fn with_syscall_mode(mut self, mode: SyscallMode) -> Self {
        self.config.syscall_mode = mode;
        self
    }

    /// Set the C compiler to use (clang or gcc).
    #[must_use]
    pub fn with_compiler(mut self, compiler: Compiler) -> Self {
        self.config.compiler = compiler;
        self
    }

    /// Suppress compilation output.
    #[must_use]
    pub const fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Enable export functions mode for calling exported functions.
    ///
    /// When enabled, all function symbols are added as CFG entry points,
    /// and `RV_EXPORT_FUNCTIONS` metadata is set in the compiled library.
    #[must_use]
    pub const fn with_export_functions(mut self, enabled: bool) -> Self {
        self.export_functions = enabled;
        self.config.export_functions = enabled;
        self
    }

    /// Get the configuration.
    #[must_use]
    pub const fn config(&self) -> &EmitConfig<X> {
        &self.config
    }

    /// Compile an ELF file to a shared library.
    ///
    /// If `jobs` is 0, auto-detects based on CPU count.
    ///
    /// # Errors
    ///
    /// Returns errors from lifting or compiling the output.
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

        let lib_path = output_dir.join(format!("lib{lib_name}.so"));
        Ok(lib_path)
    }

    /// Lift an ELF file to source code (C or x86 assembly, depending on backend).
    ///
    /// # Errors
    ///
    /// Returns errors from parsing or lifting the ELF.
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
                pipeline.lift_to_ir_as_single_blocks()?;
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
                if self.config.emit_line_info()
                    && let Some(path_str) = elf_path.to_str()
                    && let Err(e) = pipeline.load_debug_info(path_str)
                {
                    warn!(error = %e, "failed to load debug info (continuing without #line directives)");
                }

                pipeline.emit_c(output_dir, base_name)?;
                Ok(output_dir.join(format!("{base_name}_part0.c")))
            }
            Backend::X86Asm => {
                pipeline.emit_x86(output_dir, base_name)?;
                Ok(output_dir.join(format!("{base_name}.s")))
            }
            Backend::ARM64Asm => {
                pipeline.emit_arm64(output_dir, base_name)?;
                Ok(output_dir.join(format!("{base_name}.s")))
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
        Error::CompilationFailed(format!("Failed to run make: {e}"))
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
            "make failed: {first_error}"
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

fn configure_asm_command(cmd: &mut Command, needs_cross: bool, target_triple: &str) {
    if needs_cross {
        cmd.args([
            format!("--target={target_triple}"),
            "-c".to_string(),
            "-fPIC".to_string(),
        ]);
    } else {
        cmd.args(["-c", "-fPIC"]);
    }
}

fn configure_c_command(cmd: &mut Command, needs_cross: bool, target_triple: &str) {
    if needs_cross {
        cmd.args([
            format!("--target={target_triple}"),
            "-c".to_string(),
            "-fPIC".to_string(),
            "-O2".to_string(),
            "-std=c23".to_string(),
        ]);
    } else {
        cmd.args(["-c", "-fPIC", "-O2", "-std=c23"]);
    }
}

fn assemble_asm(
    cc: &str,
    asm_path: &Path,
    obj_path: &Path,
    needs_cross: bool,
    target_triple: &str,
) -> Result<()> {
    let mut asm_cmd = Command::new(cc);
    configure_asm_command(&mut asm_cmd, needs_cross, target_triple);
    asm_cmd.arg("-o").arg(obj_path).arg(asm_path);

    let asm_output = {
        let _span = info_span!("assemble").entered();
        asm_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to run {cc}: {e}")))?
    };

    if !asm_output.status.success() {
        let stderr = String::from_utf8_lossy(&asm_output.stderr);
        error!(stderr = %stderr, "assembly failed");
        let first_line = stderr.lines().next().unwrap_or("unknown error");
        return Err(Error::CompilationFailed(format!(
            "Assembly failed: {first_line}"
        )));
    }

    Ok(())
}

fn compile_optional_c(
    cc: &str,
    output_dir: &Path,
    base_name: &str,
    suffix: &str,
    needs_cross: bool,
    target_triple: &str,
) -> Result<Option<std::path::PathBuf>> {
    let c_path = output_dir.join(format!("{base_name}_{suffix}.c"));
    if !c_path.exists() {
        return Ok(None);
    }

    let obj_path = output_dir.join(format!("{base_name}_{suffix}.o"));
    let mut cmd = Command::new(cc);
    configure_c_command(&mut cmd, needs_cross, target_triple);
    cmd.arg("-I")
        .arg(output_dir)
        .arg("-o")
        .arg(&obj_path)
        .arg(&c_path);

    let output = {
        let _span = info_span!("compile_support_c", suffix = suffix).entered();
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to compile {suffix}: {e}")))?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(stderr = %stderr, "{suffix} compilation failed");
        let first_line = stderr.lines().next().unwrap_or("unknown error");
        return Err(Error::CompilationFailed(format!(
            "{suffix} compilation failed: {first_line}"
        )));
    }

    Ok(Some(obj_path))
}

fn link_shared(
    cc: &str,
    obj_files: &[std::path::PathBuf],
    lib_path: &Path,
    compiler: &Compiler,
    needs_cross: bool,
    target_triple: &str,
) -> Result<()> {
    let mut link_cmd = Command::new(cc);

    if needs_cross {
        link_cmd.args([
            format!("--target={target_triple}"),
            "-fuse-ld=lld".to_string(),
            "-nostdlib".to_string(),
            "-shared".to_string(),
            "-Wl,-z,noexecstack".to_string(),
        ]);
    } else {
        link_cmd.args(["-shared", "-Wl,-z,noexecstack"]);
        if let Some(linker) = compiler.linker() {
            link_cmd.arg(format!("-fuse-ld={linker}"));
        }
    }

    link_cmd.arg("-o").arg(lib_path);
    for obj in obj_files {
        link_cmd.arg(obj);
    }

    let link_output = {
        let _span = info_span!("link_shared").entered();
        link_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::CompilationFailed(format!("Failed to link: {e}")))?
    };

    if !link_output.status.success() {
        let stderr = String::from_utf8_lossy(&link_output.stderr);
        error!(stderr = %stderr, "linking failed");
        let first_line = stderr.lines().next().unwrap_or("unknown error");
        return Err(Error::CompilationFailed(format!(
            "Linking failed: {first_line}"
        )));
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

    let asm_path = output_dir.join(format!("{base_name}.s"));
    let obj_path = output_dir.join(format!("{base_name}.o"));
    let lib_path = output_dir.join(format!("lib{base_name}.so"));

    if !asm_path.exists() {
        return Err(Error::CompilationFailed(format!(
            "Assembly file not found: {}",
            asm_path.display()
        )));
    }

    // Check if we need cross-compilation (non-x86 host)
    let is_x86_host = cfg!(target_arch = "x86_64") || cfg!(target_arch = "x86");
    let needs_cross = !is_x86_host;

    let target_triple = "x86_64-unknown-linux-gnu";
    let cc = if needs_cross {
        "clang"
    } else {
        compiler.command()
    };

    debug!(asm = %asm_path.display(), compiler = %cc, cross = %needs_cross, "assembling");

    assemble_asm(cc, &asm_path, &obj_path, needs_cross, target_triple)?;

    debug!(obj = %obj_path.display(), "linking");

    let mut obj_files = vec![obj_path];
    if let Some(path) = compile_optional_c(
        cc,
        output_dir,
        base_name,
        "syscalls",
        needs_cross,
        target_triple,
    )? {
        obj_files.push(path);
    }
    if let Some(path) = compile_optional_c(
        cc,
        output_dir,
        base_name,
        "htif",
        needs_cross,
        target_triple,
    )? {
        obj_files.push(path);
    }

    link_shared(
        cc,
        &obj_files,
        &lib_path,
        compiler,
        needs_cross,
        target_triple,
    )?;

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

    let asm_path = output_dir.join(format!("{base_name}.s"));
    let obj_path = output_dir.join(format!("{base_name}.o"));
    let lib_path = output_dir.join(format!("lib{base_name}.so"));

    if !asm_path.exists() {
        return Err(Error::CompilationFailed(format!(
            "Assembly file not found: {}",
            asm_path.display()
        )));
    }

    let is_arm64_host = cfg!(target_arch = "aarch64");
    let needs_cross = !is_arm64_host;
    let target_triple = "aarch64-unknown-linux-gnu";
    let cc = if needs_cross {
        "clang"
    } else {
        compiler.command()
    };

    debug!(asm = %asm_path.display(), compiler = %cc, cross = %needs_cross, "assembling");

    assemble_asm(cc, &asm_path, &obj_path, needs_cross, target_triple)?;

    debug!(obj = %obj_path.display(), "linking");

    let mut obj_files = vec![obj_path];
    if let Some(path) = compile_optional_c(
        cc,
        output_dir,
        base_name,
        "syscalls",
        needs_cross,
        target_triple,
    )? {
        obj_files.push(path);
    }
    if let Some(path) = compile_optional_c(
        cc,
        output_dir,
        base_name,
        "htif",
        needs_cross,
        target_triple,
    )? {
        obj_files.push(path);
    }

    link_shared(
        cc,
        &obj_files,
        &lib_path,
        compiler,
        needs_cross,
        target_triple,
    )?;

    if !quiet {
        debug!(lib = %lib_path.display(), cross = %needs_cross, "compiled ARM64 shared library");
    }

    Ok(())
}
