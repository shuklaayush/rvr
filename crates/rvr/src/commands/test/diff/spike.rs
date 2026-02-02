//! Spike executor for differential testing.
//!
//! Spawns Spike with `--log-commits` and streams trace output via pipe.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::OnceLock;

use regex::Regex;

use super::executor::Executor;
use super::state::DiffState;

/// Executor that runs Spike and parses its commit log output.
pub struct SpikeExecutor {
    child: Child,
    reader: BufReader<ChildStdout>,
    entry_point: u64,
    instret: u64,
    current_pc: u64,
    has_exited: bool,
    exit_code: Option<u8>,
    aligned: bool,
}

impl SpikeExecutor {
    fn needs_smrnmi(elf: &Path) -> bool {
        let bytes = match std::fs::read(elf) {
            Ok(data) => data,
            Err(_) => return false,
        };

        // csrwi mnstatus, 0x8 => 0x74445073 (little-endian in ELF)
        const MNSTATUS_CSRWI: [u8; 4] = [0x73, 0x50, 0x44, 0x74];
        bytes
            .windows(MNSTATUS_CSRWI.len())
            .any(|w| w == MNSTATUS_CSRWI)
    }

    /// Start Spike with the given ELF and ISA.
    ///
    /// Uses `--log=/dev/stdout` to stream output through a pipe.
    pub fn start(elf: &Path, isa: &str, entry_point: u64) -> std::io::Result<Self> {
        let mut isa_arg = isa.to_string();
        if Self::needs_smrnmi(elf) && !isa_arg.contains("smrnmi") {
            isa_arg.push_str("_smrnmi");
        }

        let mut cmd = Command::new("spike");
        cmd.arg(format!("--isa={}", isa_arg))
            .arg("--log-commits")
            .arg("--log=/dev/stdout")
            .arg(elf)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no stdout"))?;

        Ok(Self {
            child,
            reader: BufReader::new(stdout),
            entry_point,
            instret: 0,
            current_pc: entry_point,
            has_exited: false,
            exit_code: None,
            aligned: false,
        })
    }

    /// Try to start Spike, returning None if spike is not available.
    pub fn try_start(elf: &Path, isa: &str, entry_point: u64) -> Option<Self> {
        Self::start(elf, isa, entry_point).ok()
    }

    /// Read the next instruction state from the pipe.
    fn read_next(&mut self) -> Option<DiffState> {
        let mut line = String::new();

        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF - Spike has exited
                    self.has_exited = true;
                    return None;
                }
                Ok(_) => {
                    if let Some(state) = Self::parse_line(&line) {
                        // Skip until we reach the entry point
                        if !self.aligned {
                            if state.pc >= self.entry_point {
                                self.aligned = true;
                            } else {
                                continue;
                            }
                        }
                        return Some(state);
                    }
                    // Line didn't parse - continue reading
                }
                Err(_) => {
                    self.has_exited = true;
                    return None;
                }
            }
        }
    }

    /// Parse a Spike trace line into DiffState.
    fn parse_line(line: &str) -> Option<DiffState> {
        let line = line.trim();
        if !line.starts_with("core") {
            return None;
        }

        // Parse PC and opcode: 0x<PC> (0x<OPCODE>)
        let pc_pattern = PC_PATTERN
            .get_or_init(|| Regex::new(r"0x([0-9a-fA-F]+)\s+\(0x([0-9a-fA-F]+)\)").unwrap());
        let caps = pc_pattern.captures(line)?;

        let pc = u64::from_str_radix(caps.get(1)?.as_str(), 16).ok()?;
        let opcode = u32::from_str_radix(caps.get(2)?.as_str(), 16).ok()?;

        // Parse register write: x<N> 0x<VALUE>
        let reg_pattern =
            REG_PATTERN.get_or_init(|| Regex::new(r"\bx(\d+)\s+0x([0-9a-fA-F]+)").unwrap());
        let (rd, rd_value) = if let Some(caps) = reg_pattern.captures(line) {
            let reg = caps.get(1)?.as_str().parse::<u8>().ok()?;
            let val = u64::from_str_radix(caps.get(2)?.as_str(), 16).ok()?;
            if reg == 0 {
                (None, None) // Ignore x0 writes
            } else {
                (Some(reg), Some(val))
            }
        } else {
            (None, None)
        };

        // Parse memory access: mem 0x<ADDR> [0x<VALUE>]
        let mem_pattern =
            MEM_PATTERN.get_or_init(|| Regex::new(r"\bmem\s+0x([0-9a-fA-F]+)").unwrap());
        let mem_addr = mem_pattern
            .captures(line)
            .and_then(|caps| u64::from_str_radix(caps.get(1)?.as_str(), 16).ok());

        Some(DiffState {
            pc,
            opcode,
            rd,
            rd_value,
            mem_addr,
            ..Default::default()
        })
    }
}

impl Drop for SpikeExecutor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Executor for SpikeExecutor {
    fn step(&mut self) -> Option<DiffState> {
        if self.has_exited {
            return None;
        }

        match self.read_next() {
            Some(mut state) => {
                self.instret += 1;
                state.instret = self.instret;
                self.current_pc = state.pc;
                Some(state)
            }
            None => {
                self.has_exited = true;
                None
            }
        }
    }

    fn current_pc(&self) -> u64 {
        self.current_pc
    }

    fn instret(&self) -> u64 {
        self.instret
    }

    fn has_exited(&self) -> bool {
        self.has_exited
    }

    fn exit_code(&self) -> Option<u8> {
        self.exit_code
    }
}

// Static regex patterns (shared across instances)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_basic() {
        let line = "core   0: 3 0x0000000080000050 (0x00000093) x1 0x0000000000000000";
        let state = SpikeExecutor::parse_line(line).unwrap();

        assert_eq!(state.pc, 0x80000050);
        assert_eq!(state.opcode, 0x00000093);
        assert_eq!(state.rd, Some(1));
        assert_eq!(state.rd_value, Some(0));
        assert!(state.mem_addr.is_none());
    }

    #[test]
    fn test_parse_line_with_mem() {
        let line = "core   0: 3 0x000000008000010c (0x0182b283) x5 0x0000000080000000 mem 0x0000000000001018";
        let state = SpikeExecutor::parse_line(line).unwrap();

        assert_eq!(state.pc, 0x8000010c);
        assert_eq!(state.opcode, 0x0182b283);
        assert_eq!(state.rd, Some(5));
        assert_eq!(state.rd_value, Some(0x80000000));
        assert_eq!(state.mem_addr, Some(0x1018));
    }

    #[test]
    fn test_parse_line_no_reg() {
        let line = "core   0: 3 0x0000000080000000 (0x0500006f)";
        let state = SpikeExecutor::parse_line(line).unwrap();

        assert_eq!(state.pc, 0x80000000);
        assert_eq!(state.opcode, 0x0500006f);
        assert!(state.rd.is_none());
        assert!(state.rd_value.is_none());
    }

    #[test]
    fn test_parse_line_ignores_x0() {
        let line = "core   0: 3 0x80000000 (0x00000013) x0 0x0000000000000000";
        let state = SpikeExecutor::parse_line(line).unwrap();

        assert!(state.rd.is_none());
        assert!(state.rd_value.is_none());
    }

    #[test]
    fn test_parse_line_non_trace() {
        assert!(SpikeExecutor::parse_line("some random output").is_none());
        assert!(SpikeExecutor::parse_line("").is_none());
    }
}
