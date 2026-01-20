//! ELF image with memory segments.

use rvr_isa::Xlen;

use crate::constants::*;
use crate::file::ElfFile;
use crate::header::{LoadedSection, ProgramHeader};
use crate::{ElfError, Result};

/// A memory segment with virtual address and data.
///
/// The `data` field contains only the file data (filesz bytes).
/// The total memory size is `virtual_end - virtual_start` (memsz).
/// Any bytes from `len(data)` to `memsz` are BSS (zero-initialized).
#[derive(Clone, Debug)]
pub struct MemorySegment<X: Xlen> {
    pub virtual_start: X::Reg,
    pub virtual_end: X::Reg,
    pub data: Vec<u8>,
    pub flags: u32,
}

impl<X: Xlen> MemorySegment<X> {
    /// Size of file data (non-BSS).
    pub fn filesz(&self) -> u64 {
        self.data.len() as u64
    }

    /// Total memory size including BSS.
    pub fn memsz(&self) -> u64 {
        X::to_u64(self.virtual_end) - X::to_u64(self.virtual_start)
    }

    /// Size of BSS (zero-filled) portion.
    pub fn bss_size(&self) -> u64 {
        self.memsz() - self.filesz()
    }

    /// Check if segment is read-only (no write flag).
    pub fn is_readonly(&self) -> bool {
        (self.flags & PF_W) == 0
    }

    /// Check if segment is executable.
    pub fn is_executable(&self) -> bool {
        (self.flags & PF_X) != 0
    }
}

/// ELF image ready for loading into memory.
#[derive(Clone, Debug)]
pub struct ElfImage<X: Xlen> {
    pub entry_point: X::Reg,
    pub e_flags: u32,
    pub memory_segments: Vec<MemorySegment<X>>,
    pub sections: Vec<LoadedSection<X>>,
}

impl<X: Xlen> ElfImage<X> {
    /// Parse ELF from raw bytes.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let elf = ElfFile::<X>::parse(data)?;
        let loadable = Self::validate_segments(&elf, data)?;
        let segments = Self::load_segments(&loadable, data);

        Ok(Self {
            entry_point: elf.entry_point,
            e_flags: elf.e_flags,
            memory_segments: segments,
            sections: elf.sections,
        })
    }

    /// Create ELF image from raw bytecode (not an actual ELF file).
    pub fn from_bytecode(bytecode: Vec<u8>, entry_point: X::Reg) -> Self {
        let end = X::from_u64(X::to_u64(entry_point) + bytecode.len() as u64);
        let segment = MemorySegment {
            virtual_start: entry_point,
            virtual_end: end,
            data: bytecode,
            flags: PF_X | PF_R,
        };

        Self {
            entry_point,
            e_flags: 0,
            memory_segments: vec![segment],
            sections: Vec::new(),
        }
    }

    /// Check if ELF uses the E (embedded) extension with 16 registers.
    pub fn is_rve(&self) -> bool {
        (self.e_flags & EF_RISCV_RVE) != 0
    }

    /// Check if ELF uses the C (compressed) extension.
    pub fn is_rvc(&self) -> bool {
        (self.e_flags & EF_RISCV_RVC) != 0
    }

    /// Calculate initial program break as the end of the highest memory segment.
    pub fn get_initial_program_break(&self) -> X::Reg {
        let mut max_end = X::from_u64(0);
        for segment in &self.memory_segments {
            if X::to_u64(segment.virtual_end) > X::to_u64(max_end) {
                max_end = segment.virtual_end;
            }
        }
        max_end
    }

    /// Get total loaded size (sum of all segment memsz).
    pub fn total_size(&self) -> u64 {
        self.memory_segments.iter().map(|s| s.memsz()).sum()
    }

    fn validate_segments(elf: &ElfFile<X>, file_data: &[u8]) -> Result<Vec<ProgramHeader<X>>> {
        let mut loadable = Vec::new();

        for phdr in &elf.program_headers {
            if phdr.p_type == PT_LOAD && X::to_u64(phdr.memsz) > 0 {
                let offset = X::to_u64(phdr.offset) as usize;
                let filesz = X::to_u64(phdr.filesz) as usize;

                if offset + filesz > file_data.len() {
                    return Err(ElfError::SegmentBeyondFile);
                }

                let vaddr = X::to_u64(phdr.vaddr);
                let memsz = X::to_u64(phdr.memsz);
                if vaddr.checked_add(memsz).is_none() {
                    return Err(ElfError::VirtualAddressOverflow);
                }

                loadable.push(phdr.clone());
            }
        }

        if loadable.is_empty() {
            return Err(ElfError::NoLoadableSegments);
        }

        if loadable.len() > MAX_SEGMENTS {
            return Err(ElfError::TooManySegments);
        }

        // Check for overlapping virtual ranges
        for (i, seg_i) in loadable.iter().enumerate() {
            let seg_i_start = X::to_u64(seg_i.vaddr);
            let seg_i_end = seg_i_start + X::to_u64(seg_i.memsz);

            for seg_j in loadable.iter().skip(i + 1) {
                let seg_j_start = X::to_u64(seg_j.vaddr);
                let seg_j_end = seg_j_start + X::to_u64(seg_j.memsz);

                // Check if ranges overlap
                if !(seg_i_end <= seg_j_start || seg_j_end <= seg_i_start) {
                    return Err(ElfError::OverlappingSegments);
                }
            }
        }

        Ok(loadable)
    }

    fn load_segments(
        program_headers: &[ProgramHeader<X>],
        file_data: &[u8],
    ) -> Vec<MemorySegment<X>> {
        let mut segments = Vec::with_capacity(program_headers.len());

        for phdr in program_headers {
            let offset = X::to_u64(phdr.offset) as usize;
            let filesz = X::to_u64(phdr.filesz) as usize;
            let vaddr = phdr.vaddr;
            let memsz = phdr.memsz;

            // Load file data only (BSS is handled at runtime)
            let data = file_data[offset..offset + filesz].to_vec();

            let end = X::from_u64(X::to_u64(vaddr) + X::to_u64(memsz));
            segments.push(MemorySegment {
                virtual_start: vaddr,
                virtual_end: end,
                data,
                flags: phdr.flags,
            });
        }

        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_isa::Rv64;

    #[test]
    fn test_from_bytecode() {
        let bytecode = vec![0x93, 0x00, 0x10, 0x00]; // ADDI x1, x0, 1
        let image = ElfImage::<Rv64>::from_bytecode(bytecode.clone(), 0x80000000u64);

        assert_eq!(image.entry_point, 0x80000000);
        assert_eq!(image.memory_segments.len(), 1);
        assert_eq!(image.memory_segments[0].virtual_start, 0x80000000);
        assert_eq!(image.memory_segments[0].data, bytecode);
    }

    #[test]
    fn test_segment_properties() {
        let segment = MemorySegment::<Rv64> {
            virtual_start: 0x1000,
            virtual_end: 0x2000,
            data: vec![0; 0x800], // 2KB of data
            flags: PF_R | PF_X,
        };

        assert_eq!(segment.filesz(), 0x800);
        assert_eq!(segment.memsz(), 0x1000);
        assert_eq!(segment.bss_size(), 0x800);
        assert!(segment.is_readonly());
        assert!(segment.is_executable());
    }
}
