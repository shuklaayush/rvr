//! ELF header structures.

use rvr_isa::Xlen;

/// ELF header.
#[derive(Clone, Debug)]
pub struct ElfHeader<X: Xlen> {
    pub magic: u32,
    pub class: u8,
    pub data: u8,
    pub version: u8,
    pub abi: u8,
    pub abi_version: u8,
    pub entry: X::Reg,
    pub phoff: X::Reg,
    pub shoff: X::Reg,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

/// Program header.
#[derive(Clone, Debug)]
pub struct ProgramHeader<X: Xlen> {
    pub p_type: u32,
    pub offset: X::Reg,
    pub vaddr: X::Reg,
    pub paddr: X::Reg,
    pub filesz: X::Reg,
    pub memsz: X::Reg,
    pub flags: u32,
    pub align: X::Reg,
}

/// Section header.
#[derive(Clone, Debug)]
pub struct SectionHeader<X: Xlen> {
    pub name: u32,
    pub sh_type: u32,
    pub flags: X::Reg,
    pub addr: X::Reg,
    pub offset: X::Reg,
    pub size: X::Reg,
    pub link: u32,
    pub info: u32,
    pub addralign: X::Reg,
    pub entsize: X::Reg,
}

/// Loaded section with data.
#[derive(Clone, Debug)]
pub struct LoadedSection<X: Xlen> {
    pub name: String,
    pub addr: X::Reg,
    pub size: X::Reg,
    pub data: Vec<u8>,
}
