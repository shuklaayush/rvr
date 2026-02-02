//! Trace comparison for differential testing.
//!
//! Compares instruction traces between rvr and Spike (the RISC-V reference simulator)
//! to catch bugs at the instruction level rather than just end-state.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use regex::Regex;

/// A single instruction trace entry.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceEntry {
    /// Program counter.
    pub pc: u64,
    /// Raw instruction opcode.
    pub opcode: u32,
    /// Destination register (if any).
    pub rd: Option<u8>,
    /// Value written to rd (if any).
    pub rd_value: Option<u64>,
    /// Memory address accessed (if any).
    pub mem_addr: Option<u64>,
}

impl TraceEntry {
    /// Parse a Spike-format trace line using regex.
    ///
    /// Handles various Spike output formats:
    /// - `core   0: 3 0x<PC> (0x<OPCODE>) [x<RD> 0x<VALUE>] [mem 0x<ADDR> [0x<VAL>]]`
    /// - `core   0: 0 0x<PC> (0x<OPCODE>) c<CSR>_name 0x<VALUE>`
    ///
    /// Uses pattern matching rather than positional parsing to handle format variations.
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if !line.starts_with("core") {
            return None;
        }

        let pc_pattern = PC_PATTERN
            .get_or_init(|| Regex::new(r"0x([0-9a-fA-F]+)\s+\(0x([0-9a-fA-F]+)\)").unwrap());
        let caps = pc_pattern.captures(line)?;

        let pc = u64::from_str_radix(caps.get(1)?.as_str(), 16).ok()?;
        let opcode = u32::from_str_radix(caps.get(2)?.as_str(), 16).ok()?;

        // Look for register write: x<N> 0x<VALUE>
        // Takes the FIRST register write if multiple exist
        let reg_pattern =
            REG_PATTERN.get_or_init(|| Regex::new(r"\bx(\d+)\s+0x([0-9a-fA-F]+)").unwrap());
        let (rd, rd_value) = if let Some(caps) = reg_pattern.captures(line) {
            let reg = caps.get(1)?.as_str().parse::<u8>().ok()?;
            let val = u64::from_str_radix(caps.get(2)?.as_str(), 16).ok()?;
            if reg == 0 {
                // Ignore x0 writes; Spike can log them, rvr tracer suppresses them.
                (None, None)
            } else {
                (Some(reg), Some(val))
            }
        } else {
            (None, None)
        };

        // Look for memory access: mem 0x<ADDR> [0x<VALUE>]
        // We only capture the address, value is optional and ignored
        let mem_pattern =
            MEM_PATTERN.get_or_init(|| Regex::new(r"\bmem\s+0x([0-9a-fA-F]+)").unwrap());
        let mem_addr = mem_pattern
            .captures(line)
            .and_then(|caps| u64::from_str_radix(caps.get(1)?.as_str(), 16).ok());

        Some(TraceEntry {
            pc,
            opcode,
            rd,
            rd_value,
            mem_addr,
        })
    }
}

/// Result of comparing two traces.
#[derive(Debug)]
pub struct TraceComparison {
    /// Number of instructions that matched.
    pub matched: usize,
    /// First divergence (if any).
    pub divergence: Option<TraceDivergence>,
}

/// Information about where traces diverged.
#[derive(Debug)]
pub struct TraceDivergence {
    /// Instruction index in the aligned stream where divergence occurred.
    pub index: usize,
    /// Expected entry (from Spike).
    pub expected: TraceEntry,
    /// Actual entry (from rvr).
    pub actual: TraceEntry,
    /// Type of divergence.
    pub kind: DivergenceKind,
}

/// Type of divergence between traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceKind {
    /// PC mismatch.
    Pc,
    /// Opcode mismatch.
    Opcode,
    /// Register write destination mismatch.
    RegDest,
    /// Register write value mismatch.
    RegValue,
    /// Memory address mismatch.
    MemAddr,
    /// Expected had register write, actual didn't.
    MissingRegWrite,
    /// Actual had register write, expected didn't.
    ExtraRegWrite,
    /// Expected had memory access, actual didn't.
    MissingMemAccess,
    /// Actual had memory access, expected didn't.
    ExtraMemAccess,
    /// Expected trace has remaining entries (actual ended early).
    ExpectedTail,
    /// Actual trace has remaining entries (expected ended early).
    ActualTail,
}

impl std::fmt::Display for DivergenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DivergenceKind::Pc => write!(f, "PC mismatch"),
            DivergenceKind::Opcode => write!(f, "opcode mismatch"),
            DivergenceKind::RegDest => write!(f, "register destination mismatch"),
            DivergenceKind::RegValue => write!(f, "register value mismatch"),
            DivergenceKind::MemAddr => write!(f, "memory address mismatch"),
            DivergenceKind::MissingRegWrite => write!(f, "missing register write in actual"),
            DivergenceKind::ExtraRegWrite => write!(f, "extra register write in actual"),
            DivergenceKind::MissingMemAccess => write!(f, "missing memory access in actual"),
            DivergenceKind::ExtraMemAccess => write!(f, "extra memory access in actual"),
            DivergenceKind::ExpectedTail => write!(f, "expected trace has extra tail"),
            DivergenceKind::ActualTail => write!(f, "actual trace has extra tail"),
        }
    }
}

/// Configuration for trace comparison behavior.
#[derive(Debug, Clone)]
pub struct CompareConfig {
    /// Entry point address for alignment (from ELF).
    pub entry_point: u64,
    /// Whether to require matching register writes (strict mode).
    /// If false, missing writes on one side are tolerated.
    pub strict_reg_writes: bool,
    /// Whether to require matching memory accesses (strict mode).
    /// If false, missing mem accesses on one side are tolerated.
    pub strict_mem_access: bool,
    /// Whether to stop on the first divergence.
    pub stop_on_first: bool,
}

impl Default for CompareConfig {
    fn default() -> Self {
        Self {
            entry_point: 0x80000000,
            strict_reg_writes: true,
            strict_mem_access: false, // Spike doesn't always log mem for loads
            stop_on_first: true,
        }
    }
}

/// Parse a trace file into entries.
pub fn parse_trace_file(path: &Path) -> std::io::Result<Vec<TraceEntry>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if let Some(entry) = TraceEntry::parse(&line) {
            entries.push(entry);
        }
    }

    Ok(entries)
}

/// ECALL opcode (SYSTEM instruction with funct3=0, no registers).
const ECALL_OPCODE: u32 = 0x00000073;

/// EBREAK opcode.
const EBREAK_OPCODE: u32 = 0x00100073;

/// Check if an opcode is SC.W or SC.D.
fn is_sc(opcode: u32) -> bool {
    let op = opcode & 0x7f;
    let funct5 = (opcode >> 27) & 0x1f;
    op == 0x2f && funct5 == 0b00011
}

/// Check if a PC is likely in the trap handler region.
///
/// Uses the entry point to determine: trap handlers are typically placed
/// just before or at the entry point in riscv-tests.
fn is_trap_handler_pc(pc: u64, entry_point: u64) -> bool {
    let start = entry_point.saturating_sub(0x100);
    pc >= start && pc < entry_point + 0x50
}

/// Compare two traces sequentially, tolerating missing entries.
///
/// This handles cases where one trace has instructions the other doesn't log
/// (e.g., CSR writes that Spike doesn't report or rvr executes but doesn't log).
/// When PCs don't match, we try to skip entries to resync.
///
/// Special handling for ECALL/EBREAK at end of execution:
/// - rvr handles syscalls directly and traces the ECALL instruction
/// - Spike traps to machine mode and traces the trap handler instead
/// - When rvr ends with ECALL and Spike continues with trap handler, that's expected
pub fn compare_traces_with_config(
    expected: &[TraceEntry],
    actual: &[TraceEntry],
    config: &CompareConfig,
) -> TraceComparison {
    let mut exp_idx = 0;
    let mut act_idx = 0;
    let mut matched = 0;
    let mut first_divergence: Option<TraceDivergence> = None;

    while exp_idx < expected.len() && act_idx < actual.len() {
        let exp = &expected[exp_idx];
        let act = &actual[act_idx];

        if exp.pc == act.pc {
            // Same PC - compare the instruction
            if exp.opcode != act.opcode {
                let divergence = TraceDivergence {
                    index: matched,
                    expected: exp.clone(),
                    actual: act.clone(),
                    kind: DivergenceKind::Opcode,
                };
                if config.stop_on_first {
                    return TraceComparison {
                        matched,
                        divergence: Some(divergence),
                    };
                }
                if first_divergence.is_none() {
                    first_divergence = Some(divergence);
                }
                exp_idx += 1;
                act_idx += 1;
                continue;
            }

            // Check register write presence mismatch
            if config.strict_reg_writes {
                match (exp.rd.is_some(), act.rd.is_some()) {
                    (true, false) => {
                        let divergence = TraceDivergence {
                            index: matched,
                            expected: exp.clone(),
                            actual: act.clone(),
                            kind: DivergenceKind::MissingRegWrite,
                        };
                        if config.stop_on_first {
                            return TraceComparison {
                                matched,
                                divergence: Some(divergence),
                            };
                        }
                        if first_divergence.is_none() {
                            first_divergence = Some(divergence);
                        }
                        exp_idx += 1;
                        act_idx += 1;
                        continue;
                    }
                    (false, true) => {
                        let divergence = TraceDivergence {
                            index: matched,
                            expected: exp.clone(),
                            actual: act.clone(),
                            kind: DivergenceKind::ExtraRegWrite,
                        };
                        if config.stop_on_first {
                            return TraceComparison {
                                matched,
                                divergence: Some(divergence),
                            };
                        }
                        if first_divergence.is_none() {
                            first_divergence = Some(divergence);
                        }
                        exp_idx += 1;
                        act_idx += 1;
                        continue;
                    }
                    _ => {}
                }
            }

            // Check register write values (only if both have one)
            if exp.rd.is_some() && act.rd.is_some() {
                if exp.rd != act.rd {
                    let divergence = TraceDivergence {
                        index: matched,
                        expected: exp.clone(),
                        actual: act.clone(),
                        kind: DivergenceKind::RegDest,
                    };
                    if config.stop_on_first {
                        return TraceComparison {
                            matched,
                            divergence: Some(divergence),
                        };
                    }
                    if first_divergence.is_none() {
                        first_divergence = Some(divergence);
                    }
                    exp_idx += 1;
                    act_idx += 1;
                    continue;
                }

                if exp.rd_value != act.rd_value && !is_sc(exp.opcode) {
                    let divergence = TraceDivergence {
                        index: matched,
                        expected: exp.clone(),
                        actual: act.clone(),
                        kind: DivergenceKind::RegValue,
                    };
                    if config.stop_on_first {
                        return TraceComparison {
                            matched,
                            divergence: Some(divergence),
                        };
                    }
                    if first_divergence.is_none() {
                        first_divergence = Some(divergence);
                    }
                    exp_idx += 1;
                    act_idx += 1;
                    continue;
                }
            }

            // Check memory access presence mismatch
            if config.strict_mem_access {
                match (exp.mem_addr.is_some(), act.mem_addr.is_some()) {
                    (true, false) => {
                        if is_sc(exp.opcode) {
                            // SC may or may not perform the store.
                        } else {
                            let divergence = TraceDivergence {
                                index: matched,
                                expected: exp.clone(),
                                actual: act.clone(),
                                kind: DivergenceKind::MissingMemAccess,
                            };
                            if config.stop_on_first {
                                return TraceComparison {
                                    matched,
                                    divergence: Some(divergence),
                                };
                            }
                            if first_divergence.is_none() {
                                first_divergence = Some(divergence);
                            }
                            exp_idx += 1;
                            act_idx += 1;
                            continue;
                        }
                    }
                    (false, true) => {
                        if is_sc(exp.opcode) {
                            // SC may or may not perform the store.
                        } else {
                            let divergence = TraceDivergence {
                                index: matched,
                                expected: exp.clone(),
                                actual: act.clone(),
                                kind: DivergenceKind::ExtraMemAccess,
                            };
                            if config.stop_on_first {
                                return TraceComparison {
                                    matched,
                                    divergence: Some(divergence),
                                };
                            }
                            if first_divergence.is_none() {
                                first_divergence = Some(divergence);
                            }
                            exp_idx += 1;
                            act_idx += 1;
                            continue;
                        }
                    }
                    _ => {}
                }
            }

            // Check memory address (only if both have one)
            if exp.mem_addr.is_some()
                && act.mem_addr.is_some()
                && exp.mem_addr != act.mem_addr
                && !is_sc(exp.opcode)
            {
                let divergence = TraceDivergence {
                    index: matched,
                    expected: exp.clone(),
                    actual: act.clone(),
                    kind: DivergenceKind::MemAddr,
                };
                if config.stop_on_first {
                    return TraceComparison {
                        matched,
                        divergence: Some(divergence),
                    };
                }
                if first_divergence.is_none() {
                    first_divergence = Some(divergence);
                }
                exp_idx += 1;
                act_idx += 1;
                continue;
            }

            matched += 1;
            exp_idx += 1;
            act_idx += 1;
        } else {
            // PCs don't match - try to resync by scanning ahead.
            let window = 32usize;
            let mut skip_exp = None;
            let mut skip_act = None;

            // Prefer resync on (pc, opcode) to avoid false alignment on reused PCs.
            let mut skip_exp_pc = None;
            let mut skip_act_pc = None;
            for i in 1..=window {
                if exp_idx + i < expected.len() {
                    let cand = &expected[exp_idx + i];
                    if cand.pc == act.pc && cand.opcode == act.opcode {
                        skip_exp = Some(i);
                        break;
                    }
                    if skip_exp_pc.is_none() && cand.pc == act.pc {
                        skip_exp_pc = Some(i);
                    }
                }
            }
            for i in 1..=window {
                if act_idx + i < actual.len() {
                    let cand = &actual[act_idx + i];
                    if cand.pc == exp.pc && cand.opcode == exp.opcode {
                        skip_act = Some(i);
                        break;
                    }
                    if skip_act_pc.is_none() && cand.pc == exp.pc {
                        skip_act_pc = Some(i);
                    }
                }
            }
            if skip_exp.is_none() {
                skip_exp = skip_exp_pc;
            }
            if skip_act.is_none() {
                skip_act = skip_act_pc;
            }

            if let (Some(se), Some(sa)) = (skip_exp, skip_act) {
                if se <= sa {
                    exp_idx += se;
                } else {
                    act_idx += sa;
                }
            } else if let Some(se) = skip_exp {
                exp_idx += se;
            } else if let Some(sa) = skip_act {
                act_idx += sa;
            } else {
                // Can't resync - check for expected ECALL divergence
                // When rvr traces ECALL/EBREAK and Spike traces trap handler,
                // this is expected behavior (rvr handles syscalls directly)
                let is_ecall_divergence = (act.opcode == ECALL_OPCODE
                    || act.opcode == EBREAK_OPCODE)
                    && is_trap_handler_pc(exp.pc, config.entry_point);

                if is_ecall_divergence {
                    // rvr ends with ECALL, Spike continues in trap handler
                    // This is expected - treat as success
                    matched += 1; // Count the ECALL as matched
                    return TraceComparison {
                        matched,
                        divergence: None,
                    };
                }

                // Real control flow divergence
                let divergence = TraceDivergence {
                    index: matched,
                    expected: exp.clone(),
                    actual: act.clone(),
                    kind: DivergenceKind::Pc,
                };
                if config.stop_on_first {
                    return TraceComparison {
                        matched,
                        divergence: Some(divergence),
                    };
                }
                if first_divergence.is_none() {
                    first_divergence = Some(divergence);
                }
                exp_idx += 1;
                act_idx += 1;
                continue;
            }
        }
    }

    if let Some(divergence) = first_divergence {
        return TraceComparison {
            matched,
            divergence: Some(divergence),
        };
    }

    if exp_idx < expected.len() {
        let exp = expected[exp_idx].clone();
        let act = actual.get(act_idx).cloned().unwrap_or_else(|| exp.clone());
        return TraceComparison {
            matched,
            divergence: Some(TraceDivergence {
                index: matched,
                expected: exp,
                actual: act,
                kind: DivergenceKind::ExpectedTail,
            }),
        };
    }
    if act_idx < actual.len() {
        let act = actual[act_idx].clone();
        let exp = expected
            .get(exp_idx)
            .cloned()
            .unwrap_or_else(|| act.clone());
        return TraceComparison {
            matched,
            divergence: Some(TraceDivergence {
                index: matched,
                expected: exp,
                actual: act,
                kind: DivergenceKind::ActualTail,
            }),
        };
    }

    TraceComparison {
        matched,
        divergence: None,
    }
}

/// Align traces by finding first common PC at or after the entry point.
///
/// Spike has startup code at 0x1000 before jumping to the entry point.
/// rvr starts directly at the entry point.
pub fn align_traces_at(
    spike: &[TraceEntry],
    rvr: &[TraceEntry],
    entry_point: u64,
) -> (Vec<TraceEntry>, Vec<TraceEntry>) {
    // Find first instruction at entry_point or above in Spike trace
    let spike_start = spike.iter().position(|e| e.pc >= entry_point).unwrap_or(0);

    // Find first instruction at entry_point or above in rvr trace
    let rvr_start = rvr.iter().position(|e| e.pc >= entry_point).unwrap_or(0);

    (spike[spike_start..].to_vec(), rvr[rvr_start..].to_vec())
}

/// Run a command with a timeout, returning its exit status.
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

static PC_PATTERN: OnceLock<Regex> = OnceLock::new();
static REG_PATTERN: OnceLock<Regex> = OnceLock::new();
static MEM_PATTERN: OnceLock<Regex> = OnceLock::new();

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

    // For now, assume standard extensions
    // TODO: Parse ELF attributes to determine actual extensions
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trace_entry() {
        let line = "core   0: 3 0x0000000080000050 (0x00000093) x1 0x0000000000000000";
        let entry = TraceEntry::parse(line).unwrap();

        assert_eq!(entry.pc, 0x80000050);
        assert_eq!(entry.opcode, 0x00000093);
        assert_eq!(entry.rd, Some(1));
        assert_eq!(entry.rd_value, Some(0));
        assert_eq!(entry.mem_addr, None);
    }

    #[test]
    fn test_parse_trace_entry_with_mem() {
        let line = "core   0: 3 0x000000008000010c (0x0182b283) x5 0x0000000080000000 mem 0x0000000000001018";
        let entry = TraceEntry::parse(line).unwrap();

        assert_eq!(entry.pc, 0x8000010c);
        assert_eq!(entry.opcode, 0x0182b283);
        assert_eq!(entry.rd, Some(5));
        assert_eq!(entry.rd_value, Some(0x80000000));
        assert_eq!(entry.mem_addr, Some(0x1018));
    }

    #[test]
    fn test_parse_trace_entry_with_mem_value() {
        // Spike can include memory value for stores
        let line = "core   0: 3 0x80000040 (0xfc3f2223) mem 0x80001000 0x00000001";
        let entry = TraceEntry::parse(line).unwrap();

        assert_eq!(entry.pc, 0x80000040);
        assert_eq!(entry.opcode, 0xfc3f2223);
        assert_eq!(entry.mem_addr, Some(0x80001000));
        // Value after mem addr is ignored (we only track address)
    }

    #[test]
    fn test_parse_trace_entry_no_reg() {
        let line = "core   0: 3 0x0000000080000000 (0x0500006f)";
        let entry = TraceEntry::parse(line).unwrap();

        assert_eq!(entry.pc, 0x80000000);
        assert_eq!(entry.opcode, 0x0500006f);
        assert_eq!(entry.rd, None);
        assert_eq!(entry.rd_value, None);
    }

    #[test]
    fn test_parse_trace_entry_with_csr() {
        // Spike logs CSR writes with cNNN_name format
        let line = "core   0: 3 0x800000dc (0x30529073) c773_mtvec 0x00000000800000e4";
        let entry = TraceEntry::parse(line).unwrap();

        assert_eq!(entry.pc, 0x800000dc);
        assert_eq!(entry.opcode, 0x30529073);
        // CSR write is not parsed as xN, so rd should be None
        assert_eq!(entry.rd, None);
    }

    #[test]
    fn test_parse_trace_entry_priv_level_0() {
        // Different privilege level format
        let line = "core   0: 0 0x80000200 (0x00c70733) x14 0x0000000000000337";
        let entry = TraceEntry::parse(line).unwrap();

        assert_eq!(entry.pc, 0x80000200);
        assert_eq!(entry.opcode, 0x00c70733);
        assert_eq!(entry.rd, Some(14));
        assert_eq!(entry.rd_value, Some(0x337));
    }

    #[test]
    fn test_compare_traces_match() {
        let traces = vec![
            TraceEntry {
                pc: 0x80000000,
                opcode: 0x0500006f,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
            TraceEntry {
                pc: 0x80000050,
                opcode: 0x00000093,
                rd: Some(1),
                rd_value: Some(0),
                mem_addr: None,
            },
        ];

        let result = compare_traces_with_config(&traces, &traces, &CompareConfig::default());
        assert_eq!(result.matched, 2);
        assert!(result.divergence.is_none());
    }

    #[test]
    fn test_compare_traces_missing_reg_strict() {
        let expected = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: Some(1),
            rd_value: Some(0),
            mem_addr: None,
        }];

        let actual = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: None, // Missing!
            rd_value: None,
            mem_addr: None,
        }];

        let config = CompareConfig {
            strict_reg_writes: true,
            ..Default::default()
        };
        let result = compare_traces_with_config(&expected, &actual, &config);
        assert!(result.divergence.is_some());
        assert_eq!(
            result.divergence.as_ref().unwrap().kind,
            DivergenceKind::MissingRegWrite
        );
    }

    #[test]
    fn test_compare_traces_missing_reg_lenient() {
        let expected = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: Some(1),
            rd_value: Some(0),
            mem_addr: None,
        }];

        let actual = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: None,
            rd_value: None,
            mem_addr: None,
        }];

        let config = CompareConfig {
            strict_reg_writes: false,
            ..Default::default()
        };
        let result = compare_traces_with_config(&expected, &actual, &config);
        assert!(result.divergence.is_none());
        assert_eq!(result.matched, 1);
    }

    #[test]
    fn test_compare_traces_diverge_value() {
        let expected = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: Some(1),
            rd_value: Some(0),
            mem_addr: None,
        }];

        let actual = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: Some(1),
            rd_value: Some(42), // Different!
            mem_addr: None,
        }];

        let result = compare_traces_with_config(&expected, &actual, &CompareConfig::default());
        assert!(result.divergence.is_some());
        assert_eq!(
            result.divergence.as_ref().unwrap().kind,
            DivergenceKind::RegValue
        );
    }

    #[test]
    fn test_align_traces_at_entry() {
        let spike = vec![
            TraceEntry {
                pc: 0x1000,
                opcode: 0x1,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
            TraceEntry {
                pc: 0x1004,
                opcode: 0x2,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
            TraceEntry {
                pc: 0x80000000,
                opcode: 0x3,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
        ];

        let rvr = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x3,
            rd: None,
            rd_value: None,
            mem_addr: None,
        }];

        let (aligned_spike, aligned_rvr) = align_traces_at(&spike, &rvr, 0x80000000);
        assert_eq!(aligned_spike.len(), 1);
        assert_eq!(aligned_rvr.len(), 1);
        assert_eq!(aligned_spike[0].pc, 0x80000000);
    }

    #[test]
    fn test_compare_traces_expected_tail() {
        let expected = vec![
            TraceEntry {
                pc: 0x80000000,
                opcode: 0x00000093,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
            TraceEntry {
                pc: 0x80000004,
                opcode: 0x00000013,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
        ];
        let actual = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: None,
            rd_value: None,
            mem_addr: None,
        }];

        let result = compare_traces_with_config(&expected, &actual, &CompareConfig::default());
        assert!(result.divergence.is_some());
        assert_eq!(
            result.divergence.as_ref().unwrap().kind,
            DivergenceKind::ExpectedTail
        );
    }

    #[test]
    fn test_compare_traces_actual_tail() {
        let expected = vec![TraceEntry {
            pc: 0x80000000,
            opcode: 0x00000093,
            rd: None,
            rd_value: None,
            mem_addr: None,
        }];
        let actual = vec![
            TraceEntry {
                pc: 0x80000000,
                opcode: 0x00000093,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
            TraceEntry {
                pc: 0x80000004,
                opcode: 0x00000013,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
        ];

        let result = compare_traces_with_config(&expected, &actual, &CompareConfig::default());
        assert!(result.divergence.is_some());
        assert_eq!(
            result.divergence.as_ref().unwrap().kind,
            DivergenceKind::ActualTail
        );
    }

    #[test]
    fn test_compare_traces_stop_on_first_false_records_first_divergence() {
        let expected = vec![
            TraceEntry {
                pc: 0x80000000,
                opcode: 0x00000093,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
            TraceEntry {
                pc: 0x80000004,
                opcode: 0x00000013,
                rd: Some(1),
                rd_value: Some(1),
                mem_addr: None,
            },
        ];

        let actual = vec![
            TraceEntry {
                pc: 0x80000000,
                opcode: 0x00000093,
                rd: None,
                rd_value: None,
                mem_addr: None,
            },
            TraceEntry {
                pc: 0x80000004,
                opcode: 0x00000013,
                rd: Some(1),
                rd_value: Some(2),
                mem_addr: None,
            },
        ];

        let config = CompareConfig {
            stop_on_first: false,
            ..Default::default()
        };
        let result = compare_traces_with_config(&expected, &actual, &config);
        assert!(result.divergence.is_some());
        assert_eq!(
            result.divergence.as_ref().unwrap().kind,
            DivergenceKind::RegValue
        );
    }
}
