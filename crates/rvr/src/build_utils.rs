//! Build utilities for RISC-V toolchain discovery.

use std::process::Command;

/// Find RISC-V GCC toolchain prefix.
///
/// Searches for common RISC-V GCC toolchain prefixes in PATH.
/// Returns the prefix (e.g., "riscv64-unknown-elf-") if found.
pub fn find_toolchain() -> Option<String> {
    const PREFIXES: &[&str] = &[
        "riscv64-unknown-elf-",
        "riscv32-unknown-elf-",
        "riscv64-linux-gnu-",
        "riscv32-linux-gnu-",
    ];

    for prefix in PREFIXES {
        let gcc = format!("{}gcc", prefix);
        if Command::new("which")
            .arg(&gcc)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(prefix.to_string());
        }
    }
    None
}
