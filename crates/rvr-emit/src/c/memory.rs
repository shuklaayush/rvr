//! Memory initialization code generation.
//!
//! Generates memory.c containing:
//! - Embedded ELF segment data
//! - Memory initialization with guard pages
//! - Memory cleanup function

use std::fmt::Write;

/// Memory segment information.
#[derive(Clone, Debug)]
pub struct MemorySegment {
    /// Virtual start address.
    pub vaddr: u64,
    /// File size (bytes to load from data).
    pub filesz: usize,
    /// Memory size (total size including BSS).
    pub memsz: usize,
    /// Segment data.
    pub data: Vec<u8>,
}

impl MemorySegment {
    /// Create a new memory segment.
    #[must_use]
    pub const fn new(vaddr: u64, filesz: usize, memsz: usize, data: Vec<u8>) -> Self {
        Self {
            vaddr,
            filesz,
            memsz,
            data,
        }
    }

    /// Check if segment has data to embed.
    #[must_use]
    pub const fn has_data(&self) -> bool {
        !self.data.is_empty()
    }
}

/// Memory generation configuration.
pub struct MemoryConfig {
    /// Base name for output files.
    pub base_name: String,
    /// Memory segments to embed.
    pub segments: Vec<MemorySegment>,
    /// Memory address bits.
    pub memory_bits: u8,
    /// Initial program break.
    pub initial_brk: u64,
}

impl MemoryConfig {
    /// Create memory config.
    pub fn new(
        base_name: impl Into<String>,
        segments: Vec<MemorySegment>,
        memory_bits: u8,
        initial_brk: u64,
    ) -> Self {
        Self {
            base_name: base_name.into(),
            segments,
            memory_bits,
            initial_brk,
        }
    }
}

/// Generate the memory.c file.
#[must_use]
pub fn gen_memory_file(cfg: &MemoryConfig) -> String {
    let mut s = String::new();

    // Header
    writeln!(
        s,
        r#"/* Embedded ELF memory segments */
#include "{}.h"
#include <sys/mman.h>
#include <string.h>
#include <stddef.h>

"#,
        cfg.base_name
    )
    .unwrap();

    // Generate embedded data for each segment with data
    for (i, seg) in cfg.segments.iter().enumerate() {
        if seg.has_data() {
            s.push_str(&gen_segment_data(i, seg));
        }
    }

    // Generate segment metadata table
    s.push_str(
        r"/* Segment metadata */
typedef struct {
    uint64_t vaddr;
    uint64_t filesz;
    uint64_t memsz;
    const uint8_t* data;
} Segment;

static const Segment segments[] = {
",
    );

    for (i, seg) in cfg.segments.iter().enumerate() {
        let data_ptr = if seg.has_data() {
            format!("segment_{i}_data")
        } else {
            "nullptr".to_string()
        };
        writeln!(
            s,
            "    {{ {:#x}, {}, {}, {} }},",
            seg.vaddr, seg.filesz, seg.memsz, data_ptr
        )
        .unwrap();
    }

    s.push_str("};\n\n");

    s
}

fn gen_segment_data(index: usize, seg: &MemorySegment) -> String {
    let mut s = String::new();

    writeln!(
        s,
        "/* Segment {}: {:#x} ({} bytes file, {} bytes mem) */",
        index, seg.vaddr, seg.filesz, seg.memsz
    )
    .unwrap();
    writeln!(s, "static const uint8_t segment_{index}_data[] = {{").unwrap();

    // Write data as hex bytes
    for (i, chunk) in seg.data.chunks(16).enumerate() {
        s.push_str("    ");
        for (j, byte) in chunk.iter().enumerate() {
            if j > 0 {
                s.push_str(", ");
            }
            write!(s, "{byte:#04x}").unwrap();
        }
        if i * 16 + chunk.len() < seg.data.len() {
            s.push(',');
        }
        s.push('\n');
    }

    s.push_str("};\n\n");
    s
}

/// Write binary segment files for C23 #embed directive.
/// Returns list of (filename, data) pairs.
#[must_use]
pub fn gen_segment_bins(cfg: &MemoryConfig) -> Vec<(String, Vec<u8>)> {
    cfg.segments
        .iter()
        .enumerate()
        .filter(|(_, seg)| seg.has_data())
        .map(|(i, seg)| (format!("segment_{i}.bin"), seg.data.clone()))
        .collect()
}

/// Generate memory.c using C23 #embed for large segments.
#[must_use]
pub fn gen_memory_file_with_embed(cfg: &MemoryConfig) -> String {
    let mut s = String::new();

    // Header
    writeln!(
        s,
        r#"/* Embedded ELF memory segments */
#include "{}.h"
#include <sys/mman.h>
#include <string.h>
#include <stddef.h>

"#,
        cfg.base_name
    )
    .unwrap();

    // Generate embedded data using #embed for segments with data
    for (i, seg) in cfg.segments.iter().enumerate() {
        if seg.has_data() {
            writeln!(
                s,
                r#"/* Segment {}: {:#x} ({} bytes file, {} bytes mem) */
static const uint8_t segment_{}_data[] = {{
    #embed "segment_{}.bin"
}};

"#,
                i, seg.vaddr, seg.filesz, seg.memsz, i, i
            )
            .unwrap();
        }
    }

    // Generate segment metadata table
    s.push_str(
        r"/* Segment metadata */
typedef struct {
    uint64_t vaddr;
    uint64_t filesz;
    uint64_t memsz;
    const uint8_t* data;
} Segment;

static const Segment segments[] = {
",
    );

    for (i, seg) in cfg.segments.iter().enumerate() {
        let data_ptr = if seg.has_data() {
            format!("segment_{i}_data")
        } else {
            "nullptr".to_string()
        };
        writeln!(
            s,
            "    {{ {:#x}, {}, {}, {} }},",
            seg.vaddr, seg.filesz, seg.memsz, data_ptr
        )
        .unwrap();
    }

    s.push_str("};\n\n");

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_memory() {
        let segments = vec![MemorySegment::new(
            0x8000_0000,
            16,
            32,
            vec![0x01, 0x02, 0x03, 0x04],
        )];
        let cfg = MemoryConfig::new("test", segments, 32, 0x8001_0000);
        let memory = gen_memory_file(&cfg);

        assert!(memory.contains("segment_0_data"));
        assert!(memory.contains("segments[]"));
    }

    #[test]
    fn test_gen_segment_bins() {
        let segments = vec![
            MemorySegment::new(0x8000_0000, 4, 8, vec![0x01, 0x02, 0x03, 0x04]),
            MemorySegment::new(0x9000_0000, 0, 4096, vec![]), // BSS, no data
        ];
        let cfg = MemoryConfig::new("test", segments, 32, 0x8001_0000);
        let bins = gen_segment_bins(&cfg);

        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].0, "segment_0.bin");
        assert_eq!(bins[0].1, vec![0x01, 0x02, 0x03, 0x04]);
    }
}
