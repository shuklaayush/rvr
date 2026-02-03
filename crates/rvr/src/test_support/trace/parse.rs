use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::TraceEntry;

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

static PC_PATTERN: OnceLock<Regex> = OnceLock::new();
static REG_PATTERN: OnceLock<Regex> = OnceLock::new();
static MEM_PATTERN: OnceLock<Regex> = OnceLock::new();
