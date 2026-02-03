use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

pub fn run_command_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
) -> std::io::Result<ExitStatus> {
    let mut child = cmd.spawn()?;
    let start = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "command timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Find Spike executable in PATH.
pub fn find_spike() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .filter_map(|dir| {
                let full_path = dir.join("spike");
                if full_path.is_file() {
                    Some(full_path)
                } else {
                    None
                }
            })
            .next()
    })
}

/// Determine ISA string from ELF.
pub fn elf_to_isa(elf_path: &Path) -> std::io::Result<String> {
    // Read ELF header to determine if RV32 or RV64
    let elf_data = std::fs::read(elf_path)?;

    if elf_data.len() < 5 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ELF file too small",
        ));
    }

    // Check ELF magic
    if &elf_data[0..4] != b"\x7fELF" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Not an ELF file",
        ));
    }

    // Check class (32 or 64 bit)
    let is_64 = elf_data[4] == 2;

    // Assume standard extensions when ELF attributes are unavailable.
    Ok(if is_64 {
        "rv64imac".to_string()
    } else {
        "rv32imac".to_string()
    })
}

/// Infer ISA string from riscv-tests-style filename when possible.
///
/// Examples:
/// - rv64ua-* => rv64imac_a
/// - rv64uzbb-* => rv64imac_zbb
/// - rv64uzba-* => rv64imac_zba
/// - rv64uzbs-* => rv64imac_zbs
/// - rv64si-* => rv64imac_s
pub fn isa_from_test_name(name: &str, fallback: &str) -> String {
    let mut isa = fallback.to_string();
    if name.starts_with("rv64uzba") || name.starts_with("rv32uzba") {
        isa.push_str("_zba");
    } else if name.starts_with("rv64uzbb") || name.starts_with("rv32uzbb") {
        isa.push_str("_zbb");
    } else if name.starts_with("rv64uzbs") || name.starts_with("rv32uzbs") {
        isa.push_str("_zbs");
    }
    if name.starts_with("rv64si") || name.starts_with("rv32si") {
        isa.push_str("_s");
    }
    isa
}

/// Get the entry point from an ELF file.
pub fn elf_entry_point(elf_path: &Path) -> std::io::Result<u64> {
    let elf_data = std::fs::read(elf_path)?;

    if elf_data.len() < 24 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ELF file too small",
        ));
    }

    // Check ELF magic
    if &elf_data[0..4] != b"\x7fELF" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Not an ELF file",
        ));
    }

    // Check class (32 or 64 bit)
    let is_64 = elf_data[4] == 2;

    // Entry point is at offset 0x18 for both ELF32 and ELF64
    if is_64 {
        if elf_data.len() < 0x18 + 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ELF file too small for entry point",
            ));
        }
        Ok(u64::from_le_bytes(elf_data[0x18..0x20].try_into().unwrap()))
    } else {
        if elf_data.len() < 0x18 + 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ELF file too small for entry point",
            ));
        }
        Ok(u32::from_le_bytes(elf_data[0x18..0x1c].try_into().unwrap()) as u64)
    }
}
