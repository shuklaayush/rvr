//! ELF specification constants.

// ELF header constants
pub const ELF_MAGIC: u32 = 0x464C_457F; // 0x7F 'E' 'L' 'F'
pub const ELF_CLASS_32: u8 = 1;
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_LSB: u8 = 1;
pub const ELF_VERSION_CURRENT: u8 = 1;
pub const ELF_TYPE_EXEC: u16 = 2;
pub const ELF_MACHINE_RISCV: u16 = 243;

// Program header constants
pub const PT_NULL: u32 = 0;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_NOTE: u32 = 4;
pub const PT_SHLIB: u32 = 5;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;
// TODO: explain what this is
pub const PT_GNU_STACK: u32 = 0x6474_E551;

// Program header flags
pub const PF_X: u32 = 0x1; // Execute
pub const PF_W: u32 = 0x2; // Write
pub const PF_R: u32 = 0x4; // Read

// Section header constants
pub const SHT_NULL: u32 = 0;
pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_STRTAB: u32 = 3;
pub const SHT_RELA: u32 = 4;
pub const SHT_HASH: u32 = 5;
pub const SHT_DYNAMIC: u32 = 6;
pub const SHT_NOTE: u32 = 7;
pub const SHT_NOBITS: u32 = 8;
pub const SHT_REL: u32 = 9;
pub const SHT_SHLIB: u32 = 10;
pub const SHT_DYNSYM: u32 = 11;

// Section flags
pub const SHF_WRITE: u64 = 0x1;
pub const SHF_ALLOC: u64 = 0x2;
pub const SHF_EXECINSTR: u64 = 0x4;

// Symbol binding (upper 4 bits of st_info)
pub const STB_LOCAL: u8 = 0;
pub const STB_GLOBAL: u8 = 1;
pub const STB_WEAK: u8 = 2;

// Symbol type (lower 4 bits of st_info)
pub const STT_NOTYPE: u8 = 0;
pub const STT_OBJECT: u8 = 1;
pub const STT_FUNC: u8 = 2;
pub const STT_SECTION: u8 = 3;
pub const STT_FILE: u8 = 4;

// RISC-V ELF e_flags (RISC-V ELF psABI)
pub const EF_RISCV_RVC: u32 = 0x1; // Uses C (compressed) extension
pub const EF_RISCV_FLOAT_ABI_SOFT: u32 = 0x0; // Soft-float ABI
pub const EF_RISCV_FLOAT_ABI_SINGLE: u32 = 0x2; // Single-precision float ABI
pub const EF_RISCV_FLOAT_ABI_DOUBLE: u32 = 0x4; // Double-precision float ABI
pub const EF_RISCV_FLOAT_ABI_QUAD: u32 = 0x6; // Quad-precision float ABI
pub const EF_RISCV_RVE: u32 = 0x8; // Uses E (embedded, 16 registers) extension

// Limits
// TODO: should this be higher
pub const MAX_SEGMENTS: usize = 8;
