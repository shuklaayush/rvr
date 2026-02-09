//! Debug info extraction using llvm-addr2line.
//!
//! Resolves instruction addresses to source <file:line:function> mappings
//! for generating #line directives in emitted C code.

use std::collections::HashMap;
use std::io::Write;
use std::process::Command;

use rvr_ir::SourceLoc;
use tempfile::NamedTempFile;

/// Debug info for an ELF file, mapping addresses to source locations.
#[derive(Debug, Default)]
pub struct DebugInfo {
    // TODO: fxhashmap?
    locations: HashMap<u64, SourceLoc>,
}

impl DebugInfo {
    /// Create empty debug info.
    #[must_use]
    pub fn new() -> Self {
        // TODO: only keep default
        Self::default()
    }

    /// Load debug info for a set of addresses from an ELF file.
    ///
    /// Writes addresses to a temp file and calls llvm-addr2line via shell.
    ///
    /// # Arguments
    ///
    /// * `elf_path` - Path to the ELF file.
    /// * `addresses` - Addresses to resolve.
    /// * `addr2line_cmd` - The llvm-addr2line command (e.g., "llvm-addr2line-20").
    // TODO: should this be constructor
    ///
    /// # Errors
    ///
    /// Returns an error if temp file creation, addr2line execution, or output parsing fails.
    pub fn load(elf_path: &str, addresses: &[u64], addr2line_cmd: &str) -> Result<Self, String> {
        if addresses.is_empty() {
            return Ok(Self::new());
        }

        // Write addresses to temp file (one hex address per line)
        let mut tmp =
            NamedTempFile::new().map_err(|e| format!("failed to create temp file: {e}"))?;
        for addr in addresses {
            writeln!(tmp, "0x{addr:x}").map_err(|e| format!("failed to write temp file: {e}"))?;
        }
        tmp.flush()
            .map_err(|e| format!("failed to flush temp file: {e}"))?;

        // Run llvm-addr2line via shell with file redirection
        let cmd = format!(
            "{} -e {} -f -C < {}",
            addr2line_cmd,
            elf_path,
            tmp.path().display()
        );

        let output = Command::new("sh")
            .args(["-c", &cmd])
            .output()
            .map_err(|e| format!("failed to run {addr2line_cmd}: {e}"))?;

        // tmp is automatically cleaned up when dropped

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("llvm-addr2line failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut info = Self::new();

        // Parse output: alternating function name / file:line pairs
        // TODO: do in rust idiomatic way if possible
        let lines: Vec<&str> = stdout.lines().collect();
        let mut line_idx = 0;
        let mut addr_idx = 0;

        while line_idx + 1 < lines.len() && addr_idx < addresses.len() {
            let func_line = lines[line_idx].trim();
            let loc_line = lines[line_idx + 1].trim();

            let loc = parse_location(func_line, loc_line);
            if loc.is_valid() {
                info.locations.insert(addresses[addr_idx], loc);
            }

            line_idx += 2;
            addr_idx += 1;
        }

        Ok(info)
    }

    // TODO: should be some trait impl?
    /// Get source location for an address.
    #[must_use]
    pub fn get(&self, address: u64) -> Option<&SourceLoc> {
        self.locations.get(&address)
    }

    /// Number of resolved locations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.locations.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }
}

/// Parse addr2line output into a `SourceLoc`.
fn parse_location(func_line: &str, loc_line: &str) -> SourceLoc {
    // Function name
    let function = if func_line == "??" {
        String::new()
    } else {
        func_line.to_string()
    };

    // Location: "file:line" or "file:line (discriminator N)"
    let (file, line) = loc_line.rfind(':').map_or_else(
        || (String::from("??"), 0),
        |colon_idx| {
            let file = &loc_line[..colon_idx];
            let line_part = &loc_line[colon_idx + 1..];

            // Strip discriminator if present
            let line_str = line_part.split_whitespace().next().unwrap_or("0");

            let line = line_str.parse::<u32>().unwrap_or(0);
            (file.to_string(), line)
        },
    );

    SourceLoc::new(&file, line, &function)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_location_valid() {
        let loc = parse_location("main", "/path/to/file.c:42");
        assert!(loc.is_valid());
        assert_eq!(loc.file, "/path/to/file.c");
        assert_eq!(loc.line, 42);
        assert_eq!(loc.function, "main");
    }

    #[test]
    fn test_parse_location_with_discriminator() {
        let loc = parse_location("foo", "/path/file.c:10 (discriminator 1)");
        assert!(loc.is_valid());
        assert_eq!(loc.line, 10);
    }

    #[test]
    fn test_parse_location_unknown() {
        let loc = parse_location("??", "??:0");
        assert!(!loc.is_valid());
    }
}
