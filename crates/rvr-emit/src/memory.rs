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
    pub fn new(vaddr: u64, filesz: usize, memsz: usize, data: Vec<u8>) -> Self {
        Self {
            vaddr,
            filesz,
            memsz,
            data,
        }
    }

    /// Check if segment has data to embed.
    pub fn has_data(&self) -> bool {
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
}

impl MemoryConfig {
    /// Create memory config.
    pub fn new(
        base_name: impl Into<String>,
        segments: Vec<MemorySegment>,
        memory_bits: u8,
    ) -> Self {
        Self {
            base_name: base_name.into(),
            segments,
            memory_bits,
        }
    }
}

/// Generate the memory.c file.
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
        r#"/* Segment metadata */
typedef struct {
    uint64_t vaddr;
    uint64_t filesz;
    uint64_t memsz;
    const uint8_t* data;
} Segment;

static const Segment segments[] = {
"#,
    );

    for (i, seg) in cfg.segments.iter().enumerate() {
        let data_ptr = if seg.has_data() {
            format!("segment_{}_data", i)
        } else {
            "NULL".to_string()
        };
        writeln!(
            s,
            "    {{ {:#x}, {}, {}, {} }},",
            seg.vaddr, seg.filesz, seg.memsz, data_ptr
        )
        .unwrap();
    }

    s.push_str("};\n\n");

    // Memory init/free functions
    s.push_str(&gen_memory_functions(cfg));

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
    writeln!(s, "static const uint8_t segment_{}_data[] = {{", index).unwrap();

    // Write data as hex bytes
    for (i, chunk) in seg.data.chunks(16).enumerate() {
        s.push_str("    ");
        for (j, byte) in chunk.iter().enumerate() {
            if j > 0 {
                s.push_str(", ");
            }
            write!(s, "{:#04x}", byte).unwrap();
        }
        if i * 16 + chunk.len() < seg.data.len() {
            s.push(',');
        }
        s.push('\n');
    }

    s.push_str("};\n\n");
    s
}

fn gen_memory_functions(_cfg: &MemoryConfig) -> String {
    r#"/* Guard size (>= page size and max load/store offset +/-2048) */
constexpr size_t GUARD_SIZE = 1 << 14;

void rv_init_memory(RvState* state) {
    size_t total_size = RV_MEMORY_SIZE + 2 * GUARD_SIZE;

    /* Allocate with guard pages on each side */
    uint8_t* region = (uint8_t*)mmap(NULL, total_size,
        PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
    if (region == MAP_FAILED) {
        perror("mmap");
        exit(1);
    }
    state->memory = region + GUARD_SIZE;
    mprotect(state->memory, RV_MEMORY_SIZE, PROT_READ | PROT_WRITE);

    /* Copy segment data */
    for (size_t i = 0; i < sizeof(segments)/sizeof(segments[0]); i++) {
        if (segments[i].data != NULL && segments[i].filesz > 0) {
            memcpy(state->memory + segments[i].vaddr,
                   segments[i].data, segments[i].filesz);
        }
    }
}

void rv_free_memory(RvState* state) {
    if (state->memory != NULL) {
        munmap(state->memory - GUARD_SIZE, RV_MEMORY_SIZE + 2 * GUARD_SIZE);
        state->memory = NULL;
    }
}
"#
    .to_string()
}

/// Write binary segment files for C23 #embed directive.
/// Returns list of (filename, data) pairs.
pub fn gen_segment_bins(cfg: &MemoryConfig) -> Vec<(String, Vec<u8>)> {
    cfg.segments
        .iter()
        .enumerate()
        .filter(|(_, seg)| seg.has_data())
        .map(|(i, seg)| (format!("segment_{}.bin", i), seg.data.clone()))
        .collect()
}

/// Generate memory.c using C23 #embed for large segments.
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
        r#"/* Segment metadata */
typedef struct {
    uint64_t vaddr;
    uint64_t filesz;
    uint64_t memsz;
    const uint8_t* data;
} Segment;

static const Segment segments[] = {
"#,
    );

    for (i, seg) in cfg.segments.iter().enumerate() {
        let data_ptr = if seg.has_data() {
            format!("segment_{}_data", i)
        } else {
            "NULL".to_string()
        };
        writeln!(
            s,
            "    {{ {:#x}, {}, {}, {} }},",
            seg.vaddr, seg.filesz, seg.memsz, data_ptr
        )
        .unwrap();
    }

    s.push_str("};\n\n");

    // Memory init/free functions
    s.push_str(&gen_memory_functions(cfg));

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_memory() {
        let segments = vec![MemorySegment::new(
            0x80000000,
            16,
            32,
            vec![0x01, 0x02, 0x03, 0x04],
        )];
        let cfg = MemoryConfig::new("test", segments, 32);
        let memory = gen_memory_file(&cfg);

        assert!(memory.contains("segment_0_data"));
        assert!(memory.contains("rv_init_memory"));
        assert!(memory.contains("rv_free_memory"));
        assert!(memory.contains("GUARD_SIZE"));
    }

    #[test]
    fn test_gen_segment_bins() {
        let segments = vec![
            MemorySegment::new(0x80000000, 4, 8, vec![0x01, 0x02, 0x03, 0x04]),
            MemorySegment::new(0x90000000, 0, 4096, vec![]), // BSS, no data
        ];
        let cfg = MemoryConfig::new("test", segments, 32);
        let bins = gen_segment_bins(&cfg);

        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0].0, "segment_0.bin");
        assert_eq!(bins[0].1, vec![0x01, 0x02, 0x03, 0x04]);
    }
}
