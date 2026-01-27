//! ELF file parser.

use rvr_isa::Xlen;

use crate::constants::*;
use crate::header::*;
use crate::{ElfError, Result};

/// Read little-endian u16 from bytes.
#[inline]
fn read_le16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

/// Read little-endian u32 from bytes.
#[inline]
fn read_le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Read little-endian u64 from bytes.
#[inline]
fn read_le64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ])
}

/// Parsed ELF file.
#[derive(Clone, Debug)]
pub struct ElfFile<X: Xlen> {
    pub entry_point: X::Reg,
    pub e_flags: u32,
    pub sections: Vec<LoadedSection<X>>,
    pub program_headers: Vec<ProgramHeader<X>>,
    pub symbols: Vec<Symbol<X>>,
}

impl<X: Xlen> ElfFile<X> {
    /// Parse ELF file from raw bytes.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let header = Self::parse_header(data)?;

        // Verify XLEN matches
        let elf_xlen = if header.class == ELF_CLASS_64 { 64 } else { 32 };
        if elf_xlen != X::VALUE {
            return Err(ElfError::XlenMismatch {
                expected: X::VALUE,
                actual: elf_xlen,
            });
        }

        let program_headers = Self::parse_program_headers(data, &header)?;
        let all_sections = Self::parse_all_sections(data, &header)?;
        let strtab = Self::find_string_table(&all_sections, &header);
        let sections = Self::load_allocatable_sections(data, &all_sections, strtab.as_ref());
        let symbols = Self::parse_symbols(data, &all_sections);

        Ok(Self {
            entry_point: header.entry,
            e_flags: header.flags,
            sections,
            program_headers,
            symbols,
        })
    }

    /// Look up a symbol by name.
    ///
    /// Returns the symbol's value (address) if found.
    pub fn lookup_symbol(&self, name: &str) -> Option<u64> {
        self.symbols
            .iter()
            .find(|s| s.name == name)
            .map(|s| X::to_u64(s.value))
    }

    /// Look up a function symbol by name.
    ///
    /// Only returns symbols with STT_FUNC type.
    pub fn lookup_function(&self, name: &str) -> Option<u64> {
        self.symbols
            .iter()
            .find(|s| s.name == name && s.sym_type == STT_FUNC)
            .map(|s| X::to_u64(s.value))
    }

    /// Check if ELF uses the E (embedded) extension with 16 registers.
    pub fn is_rve(&self) -> bool {
        (self.e_flags & EF_RISCV_RVE) != 0
    }

    /// Check if ELF uses the C (compressed) extension.
    pub fn is_rvc(&self) -> bool {
        (self.e_flags & EF_RISCV_RVC) != 0
    }

    fn parse_header(data: &[u8]) -> Result<ElfHeader<X>> {
        let min_size = if X::VALUE == 64 { 64 } else { 52 };
        if data.len() < min_size {
            return Err(ElfError::TooSmall);
        }

        // Check for Git LFS pointer (happens when `git lfs pull` wasn't run)
        if is_git_lfs_pointer(data) {
            return Err(ElfError::GitLfsPointer);
        }

        let magic = read_le32(data, 0);
        if magic != ELF_MAGIC {
            return Err(ElfError::InvalidMagic);
        }

        let class = data[4];
        let data_encoding = data[5];
        let version = data[6];
        let abi = data[7];
        let abi_version = data[8];

        if data_encoding != ELF_DATA_LSB {
            return Err(ElfError::NotLittleEndian);
        }

        if X::VALUE == 64 {
            Self::parse_header_64(data, magic, class, data_encoding, version, abi, abi_version)
        } else {
            Self::parse_header_32(data, magic, class, data_encoding, version, abi, abi_version)
        }
    }

    fn parse_header_32(
        data: &[u8],
        magic: u32,
        class: u8,
        data_encoding: u8,
        version: u8,
        abi: u8,
        abi_version: u8,
    ) -> Result<ElfHeader<X>> {
        Ok(ElfHeader {
            magic,
            class,
            data: data_encoding,
            version,
            abi,
            abi_version,
            entry: X::from_u64(read_le32(data, 24) as u64),
            phoff: X::from_u64(read_le32(data, 28) as u64),
            shoff: X::from_u64(read_le32(data, 32) as u64),
            flags: read_le32(data, 36),
            ehsize: read_le16(data, 40),
            phentsize: read_le16(data, 42),
            phnum: read_le16(data, 44),
            shentsize: read_le16(data, 46),
            shnum: read_le16(data, 48),
            shstrndx: read_le16(data, 50),
        })
    }

    fn parse_header_64(
        data: &[u8],
        magic: u32,
        class: u8,
        data_encoding: u8,
        version: u8,
        abi: u8,
        abi_version: u8,
    ) -> Result<ElfHeader<X>> {
        Ok(ElfHeader {
            magic,
            class,
            data: data_encoding,
            version,
            abi,
            abi_version,
            entry: X::from_u64(read_le64(data, 24)),
            phoff: X::from_u64(read_le64(data, 32)),
            shoff: X::from_u64(read_le64(data, 40)),
            flags: read_le32(data, 48),
            ehsize: read_le16(data, 52),
            phentsize: read_le16(data, 54),
            phnum: read_le16(data, 56),
            shentsize: read_le16(data, 58),
            shnum: read_le16(data, 60),
            shstrndx: read_le16(data, 62),
        })
    }

    fn parse_program_headers(data: &[u8], header: &ElfHeader<X>) -> Result<Vec<ProgramHeader<X>>> {
        let mut headers = Vec::with_capacity(header.phnum as usize);

        for i in 0..header.phnum {
            let offset =
                X::to_u64(header.phoff) as usize + (i as usize) * (header.phentsize as usize);
            let ph = Self::parse_program_header(data, offset)?;
            headers.push(ph);
        }

        Ok(headers)
    }

    fn parse_program_header(data: &[u8], offset: usize) -> Result<ProgramHeader<X>> {
        if X::VALUE == 64 {
            if offset + 56 > data.len() {
                return Err(ElfError::ProgramOutOfBounds);
            }
            Ok(ProgramHeader {
                p_type: read_le32(data, offset),
                flags: read_le32(data, offset + 4),
                offset: X::from_u64(read_le64(data, offset + 8)),
                vaddr: X::from_u64(read_le64(data, offset + 16)),
                paddr: X::from_u64(read_le64(data, offset + 24)),
                filesz: X::from_u64(read_le64(data, offset + 32)),
                memsz: X::from_u64(read_le64(data, offset + 40)),
                align: X::from_u64(read_le64(data, offset + 48)),
            })
        } else {
            if offset + 32 > data.len() {
                return Err(ElfError::ProgramOutOfBounds);
            }
            Ok(ProgramHeader {
                p_type: read_le32(data, offset),
                offset: X::from_u64(read_le32(data, offset + 4) as u64),
                vaddr: X::from_u64(read_le32(data, offset + 8) as u64),
                paddr: X::from_u64(read_le32(data, offset + 12) as u64),
                filesz: X::from_u64(read_le32(data, offset + 16) as u64),
                memsz: X::from_u64(read_le32(data, offset + 20) as u64),
                flags: read_le32(data, offset + 24),
                align: X::from_u64(read_le32(data, offset + 28) as u64),
            })
        }
    }

    fn parse_all_sections(data: &[u8], header: &ElfHeader<X>) -> Result<Vec<SectionHeader<X>>> {
        let mut sections = Vec::with_capacity(header.shnum as usize);

        for i in 0..header.shnum {
            let offset =
                X::to_u64(header.shoff) as usize + (i as usize) * (header.shentsize as usize);
            let sh = Self::parse_section_header(data, offset)?;
            sections.push(sh);
        }

        Ok(sections)
    }

    fn parse_section_header(data: &[u8], offset: usize) -> Result<SectionHeader<X>> {
        if X::VALUE == 64 {
            if offset + 64 > data.len() {
                return Err(ElfError::SectionOutOfBounds);
            }
            Ok(SectionHeader {
                name: read_le32(data, offset),
                sh_type: read_le32(data, offset + 4),
                flags: X::from_u64(read_le64(data, offset + 8)),
                addr: X::from_u64(read_le64(data, offset + 16)),
                offset: X::from_u64(read_le64(data, offset + 24)),
                size: X::from_u64(read_le64(data, offset + 32)),
                link: read_le32(data, offset + 40),
                info: read_le32(data, offset + 44),
                addralign: X::from_u64(read_le64(data, offset + 48)),
                entsize: X::from_u64(read_le64(data, offset + 56)),
            })
        } else {
            if offset + 40 > data.len() {
                return Err(ElfError::SectionOutOfBounds);
            }
            Ok(SectionHeader {
                name: read_le32(data, offset),
                sh_type: read_le32(data, offset + 4),
                flags: X::from_u64(read_le32(data, offset + 8) as u64),
                addr: X::from_u64(read_le32(data, offset + 12) as u64),
                offset: X::from_u64(read_le32(data, offset + 16) as u64),
                size: X::from_u64(read_le32(data, offset + 20) as u64),
                link: read_le32(data, offset + 24),
                info: read_le32(data, offset + 28),
                addralign: X::from_u64(read_le32(data, offset + 32) as u64),
                entsize: X::from_u64(read_le32(data, offset + 36) as u64),
            })
        }
    }

    fn find_string_table(
        sections: &[SectionHeader<X>],
        header: &ElfHeader<X>,
    ) -> Option<SectionHeader<X>> {
        let idx = header.shstrndx as usize;
        if idx < sections.len() {
            Some(sections[idx].clone())
        } else {
            None
        }
    }

    fn load_allocatable_sections(
        data: &[u8],
        sections: &[SectionHeader<X>],
        strtab: Option<&SectionHeader<X>>,
    ) -> Vec<LoadedSection<X>> {
        let mut loaded = Vec::new();

        for section in sections {
            // Load sections with SHF_ALLOC flag
            if (X::to_u64(section.flags) & SHF_ALLOC) != 0 {
                let section_data = Self::load_section_data(data, section);
                let name = if let Some(strtab) = strtab {
                    Self::extract_string(
                        data,
                        X::to_u64(strtab.offset) as usize,
                        section.name as usize,
                    )
                } else {
                    "unknown".to_string()
                };

                loaded.push(LoadedSection {
                    name,
                    addr: section.addr,
                    size: section.size,
                    flags: X::to_u64(section.flags),
                    data: section_data,
                });
            }
        }

        loaded
    }

    fn load_section_data(data: &[u8], section: &SectionHeader<X>) -> Vec<u8> {
        let size = X::to_u64(section.size) as usize;
        let offset = X::to_u64(section.offset) as usize;

        match section.sh_type {
            SHT_PROGBITS => {
                // Sections with file data
                let mut section_data = Vec::with_capacity(size);
                for i in 0..size {
                    let data_offset = offset + i;
                    if data_offset < data.len() {
                        section_data.push(data[data_offset]);
                    } else {
                        section_data.push(0);
                    }
                }
                section_data
            }
            SHT_NOBITS => {
                // BSS - zero-filled
                vec![0u8; size]
            }
            _ => Vec::new(),
        }
    }

    fn extract_string(data: &[u8], strtab_offset: usize, string_offset: usize) -> String {
        let start = strtab_offset + string_offset;
        let mut result = String::new();

        if start >= data.len() {
            return result;
        }

        for &byte in &data[start..] {
            if byte == 0 {
                break;
            }
            result.push(byte as char);
        }

        result
    }

    /// Parse symbol table from ELF sections.
    fn parse_symbols(data: &[u8], sections: &[SectionHeader<X>]) -> Vec<Symbol<X>> {
        let mut symbols = Vec::new();

        // Find symbol table section (.symtab)
        let symtab = sections.iter().find(|s| s.sh_type == SHT_SYMTAB);
        let symtab = match symtab {
            Some(s) => s,
            None => return symbols,
        };

        // Find string table for symbol names (linked via sh_link)
        let strtab_idx = symtab.link as usize;
        let strtab = if strtab_idx < sections.len() {
            &sections[strtab_idx]
        } else {
            return symbols;
        };

        let strtab_offset = X::to_u64(strtab.offset) as usize;
        let symtab_offset = X::to_u64(symtab.offset) as usize;
        let symtab_size = X::to_u64(symtab.size) as usize;
        let entsize = X::to_u64(symtab.entsize) as usize;

        if entsize == 0 {
            return symbols;
        }

        let num_symbols = symtab_size / entsize;

        for i in 0..num_symbols {
            let offset = symtab_offset + i * entsize;
            if let Some(sym) = Self::parse_symbol(data, offset, strtab_offset) {
                symbols.push(sym);
            }
        }

        symbols
    }

    /// Parse a single symbol entry.
    fn parse_symbol(data: &[u8], offset: usize, strtab_offset: usize) -> Option<Symbol<X>> {
        if X::VALUE == 64 {
            // ELF64 symbol: 24 bytes
            if offset + 24 > data.len() {
                return None;
            }

            let name_idx = read_le32(data, offset) as usize;
            let info = data[offset + 4];
            let _other = data[offset + 5];
            let shndx = read_le16(data, offset + 6);
            let value = X::from_u64(read_le64(data, offset + 8));
            let size = X::from_u64(read_le64(data, offset + 16));

            let name = Self::extract_string(data, strtab_offset, name_idx);
            let sym_type = info & 0xf;
            let binding = info >> 4;

            Some(Symbol {
                name,
                value,
                size,
                sym_type,
                binding,
                shndx,
            })
        } else {
            // ELF32 symbol: 16 bytes
            if offset + 16 > data.len() {
                return None;
            }

            let name_idx = read_le32(data, offset) as usize;
            let value = X::from_u64(read_le32(data, offset + 4) as u64);
            let size = X::from_u64(read_le32(data, offset + 8) as u64);
            let info = data[offset + 12];
            let _other = data[offset + 13];
            let shndx = read_le16(data, offset + 14);

            let name = Self::extract_string(data, strtab_offset, name_idx);
            let sym_type = info & 0xf;
            let binding = info >> 4;

            Some(Symbol {
                name,
                value,
                size,
                sym_type,
                binding,
                shndx,
            })
        }
    }
}

/// Git LFS pointer file prefix.
const GIT_LFS_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1";

/// Check if data looks like a Git LFS pointer file.
fn is_git_lfs_pointer(data: &[u8]) -> bool {
    data.len() >= GIT_LFS_PREFIX.len() && data.starts_with(GIT_LFS_PREFIX)
}

/// Peek at ELF header to determine XLEN (32 or 64) without full parsing.
pub fn get_elf_xlen(data: &[u8]) -> Result<u8> {
    if data.len() < 5 {
        return Err(ElfError::TooSmall);
    }

    // Check for Git LFS pointer (happens when `git lfs pull` wasn't run)
    if is_git_lfs_pointer(data) {
        return Err(ElfError::GitLfsPointer);
    }

    // Validate magic
    if data[0] != 0x7F || data[1] != 0x45 || data[2] != 0x4C || data[3] != 0x46 {
        return Err(ElfError::InvalidMagic);
    }

    // Read EI_CLASS
    match data[4] {
        ELF_CLASS_32 => Ok(32),
        ELF_CLASS_64 => Ok(64),
        other => Err(ElfError::UnsupportedClass(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_elf_xlen_32() {
        let data = [0x7F, 0x45, 0x4C, 0x46, 0x01, 0x01, 0x01, 0x00];
        assert_eq!(get_elf_xlen(&data).unwrap(), 32);
    }

    #[test]
    fn test_get_elf_xlen_64() {
        let data = [0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01, 0x01, 0x00];
        assert_eq!(get_elf_xlen(&data).unwrap(), 64);
    }

    #[test]
    fn test_invalid_magic() {
        let data = [0x00, 0x00, 0x00, 0x00, 0x02];
        assert!(matches!(get_elf_xlen(&data), Err(ElfError::InvalidMagic)));
    }

    #[test]
    fn test_git_lfs_pointer() {
        let data = b"version https://git-lfs.github.com/spec/v1\noid sha256:abc123\nsize 12345\n";
        assert!(matches!(get_elf_xlen(data), Err(ElfError::GitLfsPointer)));
    }
}
