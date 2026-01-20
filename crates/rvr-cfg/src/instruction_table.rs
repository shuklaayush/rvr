//! Instruction table for decoded instructions.
//!
//! Maintains decoded instructions, sizes, raw opcodes, and valid mask.
//! Based on Mojo's InstructionTable.

use rvr_isa::{DecodedInstr, ExtensionRegistry, Xlen};

/// Read-only memory segment for constant propagation.
#[derive(Clone, Debug)]
pub struct RoSegment {
    /// Start address.
    pub start: u64,
    /// End address (exclusive).
    pub end: u64,
    /// Segment data.
    pub data: Vec<u8>,
}

impl RoSegment {
    pub fn new(start: u64, end: u64, data: Vec<u8>) -> Self {
        Self { start, end, data }
    }

    /// Check if address is within this segment.
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }

    /// Read a value from this segment (little-endian, up to 8 bytes).
    pub fn read(&self, addr: u64, size: usize) -> Option<u64> {
        if !self.contains(addr) || addr + size as u64 > self.end {
            return None;
        }
        let offset = (addr - self.start) as usize;
        if offset + size > self.data.len() {
            return None;
        }
        let mut value = 0u64;
        for i in 0..size {
            value |= (self.data[offset + i] as u64) << (i * 8);
        }
        Some(value)
    }
}

/// Table of decoded instructions.
///
/// Maintains instruction slots indexed by PC, with 2-byte slot size.
/// Handles both compressed (2-byte) and full (4-byte) instructions.
pub struct InstructionTable<X: Xlen> {
    /// Decoded instructions (indexed by slot).
    instructions: Vec<Option<DecodedInstr<X>>>,
    /// Valid mask (true if slot contains a valid instruction start).
    valid_mask: Vec<bool>,
    /// Instruction sizes (2 or 4 bytes, 0 for invalid).
    instruction_sizes: Vec<u8>,
    /// Raw opcodes (up to 4 bytes per slot).
    raw_opcodes: Vec<u32>,
    /// Base address of the table.
    base_address: u64,
    /// End address (exclusive).
    end_address: u64,
    /// Entry point address.
    entry_point: u64,
    /// Read-only segments for constant propagation.
    ro_segments: Vec<RoSegment>,
    /// Slot size in bytes (always 2 for RISC-V with C extension).
    slot_size: usize,
}

impl<X: Xlen> InstructionTable<X> {
    /// Slot size in bytes (2 for RISC-V with C extension support).
    pub const SLOT_SIZE: usize = 2;

    /// Create a new instruction table from raw bytes.
    pub fn from_bytes(
        code: &[u8],
        base_address: u64,
        registry: &ExtensionRegistry<X>,
    ) -> Self {
        let end_address = base_address + code.len() as u64;
        let total_slots = (code.len() + Self::SLOT_SIZE - 1) / Self::SLOT_SIZE;

        let mut table = Self {
            instructions: vec![None; total_slots],
            valid_mask: vec![false; total_slots],
            instruction_sizes: vec![0; total_slots],
            raw_opcodes: vec![0; total_slots],
            base_address,
            end_address,
            entry_point: base_address,
            ro_segments: vec![RoSegment::new(base_address, end_address, code.to_vec())],
            slot_size: Self::SLOT_SIZE,
        };

        table.decode_all(code, 0, registry);
        table
    }

    /// Create a new instruction table with specific address range.
    pub fn new(base_address: u64, end_address: u64, entry_point: u64) -> Self {
        let total_size = (end_address - base_address) as usize;
        let total_slots = (total_size + Self::SLOT_SIZE - 1) / Self::SLOT_SIZE;

        Self {
            instructions: vec![None; total_slots],
            valid_mask: vec![false; total_slots],
            instruction_sizes: vec![0; total_slots],
            raw_opcodes: vec![0; total_slots],
            base_address,
            end_address,
            entry_point,
            ro_segments: Vec::new(),
            slot_size: Self::SLOT_SIZE,
        }
    }

    /// Decode all instructions from code at given slot offset.
    fn decode_all(&mut self, code: &[u8], start_slot: usize, registry: &ExtensionRegistry<X>) {
        let mut offset = 0;

        while offset + 2 <= code.len() {
            let pc = self.base_address + (start_slot * self.slot_size + offset) as u64;
            let slot = start_slot + offset / self.slot_size;

            if slot >= self.instructions.len() {
                break;
            }

            // Decode instruction
            if let Some(instr) = registry.decode(&code[offset..], X::from_u64(pc)) {
                let size = instr.size as usize;
                self.instructions[slot] = Some(instr.clone());
                self.valid_mask[slot] = true;
                self.instruction_sizes[slot] = size as u8;

                // Store raw opcode
                if size == 2 {
                    self.raw_opcodes[slot] = u16::from_le_bytes([code[offset], code[offset + 1]]) as u32;
                } else if size == 4 && offset + 4 <= code.len() {
                    self.raw_opcodes[slot] = u32::from_le_bytes([
                        code[offset],
                        code[offset + 1],
                        code[offset + 2],
                        code[offset + 3],
                    ]);
                }

                // Mark second slot as invalid for 4-byte instructions
                if size == 4 && slot + 1 < self.instructions.len() {
                    self.instructions[slot + 1] = None;
                    self.valid_mask[slot + 1] = false;
                    self.instruction_sizes[slot + 1] = 0;
                    self.raw_opcodes[slot + 1] = 0;
                }

                offset += size;
            } else {
                // Invalid instruction - skip 2 bytes
                offset += 2;
            }
        }
    }

    /// Populate from a segment of code at a specific address.
    pub fn populate_segment(
        &mut self,
        code: &[u8],
        segment_start: u64,
        registry: &ExtensionRegistry<X>,
    ) {
        if segment_start < self.base_address || segment_start >= self.end_address {
            return;
        }

        let start_slot = ((segment_start - self.base_address) / self.slot_size as u64) as usize;
        self.decode_segment(code, start_slot, segment_start, registry);
    }

    /// Decode instructions from a segment.
    fn decode_segment(
        &mut self,
        code: &[u8],
        start_slot: usize,
        segment_start: u64,
        registry: &ExtensionRegistry<X>,
    ) {
        let mut offset = 0;

        while offset + 2 <= code.len() {
            let pc = segment_start + offset as u64;
            let slot = start_slot + offset / self.slot_size;

            if slot >= self.instructions.len() {
                break;
            }

            if let Some(instr) = registry.decode(&code[offset..], X::from_u64(pc)) {
                let size = instr.size as usize;
                self.instructions[slot] = Some(instr);
                self.valid_mask[slot] = true;
                self.instruction_sizes[slot] = size as u8;

                if size == 2 {
                    self.raw_opcodes[slot] = u16::from_le_bytes([code[offset], code[offset + 1]]) as u32;
                } else if size == 4 && offset + 4 <= code.len() {
                    self.raw_opcodes[slot] = u32::from_le_bytes([
                        code[offset],
                        code[offset + 1],
                        code[offset + 2],
                        code[offset + 3],
                    ]);
                }

                if size == 4 && slot + 1 < self.instructions.len() {
                    self.instructions[slot + 1] = None;
                    self.valid_mask[slot + 1] = false;
                    self.instruction_sizes[slot + 1] = 0;
                    self.raw_opcodes[slot + 1] = 0;
                }

                offset += size;
            } else {
                offset += 2;
            }
        }
    }

    /// Add a read-only segment for constant propagation.
    pub fn add_ro_segment(&mut self, start: u64, end: u64, data: Vec<u8>) {
        self.ro_segments.push(RoSegment::new(start, end, data));
    }

    // ============= Accessors =============

    /// Get base address.
    pub fn base_address(&self) -> u64 {
        self.base_address
    }

    /// Get end address (exclusive).
    pub fn end_address(&self) -> u64 {
        self.end_address
    }

    /// Get entry point.
    pub fn entry_point(&self) -> u64 {
        self.entry_point
    }

    /// Set entry point.
    pub fn set_entry_point(&mut self, entry_point: u64) {
        self.entry_point = entry_point;
    }

    /// Get total number of slots.
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Check if table is empty.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    /// Convert PC to slot index.
    pub fn pc_to_index(&self, pc: u64) -> Option<usize> {
        if pc < self.base_address || pc >= self.end_address {
            return None;
        }
        let offset = (pc - self.base_address) as usize;
        if offset % self.slot_size != 0 {
            return None;
        }
        Some(offset / self.slot_size)
    }

    /// Convert slot index to PC.
    pub fn index_to_pc(&self, index: usize) -> u64 {
        self.base_address + (index * self.slot_size) as u64
    }

    /// Check if slot is valid.
    pub fn is_valid_index(&self, index: usize) -> bool {
        index < self.valid_mask.len() && self.valid_mask[index]
    }

    /// Check if PC points to a valid instruction.
    pub fn is_valid_pc(&self, pc: u64) -> bool {
        self.pc_to_index(pc)
            .map(|idx| self.is_valid_index(idx))
            .unwrap_or(false)
    }

    /// Get instruction at slot index.
    pub fn get(&self, index: usize) -> Option<&DecodedInstr<X>> {
        self.instructions.get(index).and_then(|i| i.as_ref())
    }

    /// Get instruction at PC.
    pub fn get_at_pc(&self, pc: u64) -> Option<&DecodedInstr<X>> {
        self.pc_to_index(pc).and_then(|idx| self.get(idx))
    }

    /// Get instruction size at slot index.
    pub fn instruction_size(&self, index: usize) -> u8 {
        self.instruction_sizes.get(index).copied().unwrap_or(0)
    }

    /// Get instruction size at PC.
    pub fn instruction_size_at_pc(&self, pc: u64) -> u8 {
        self.pc_to_index(pc)
            .map(|idx| self.instruction_size(idx))
            .unwrap_or(0)
    }

    /// Get raw opcode at slot index.
    pub fn raw_opcode(&self, index: usize) -> u32 {
        self.raw_opcodes.get(index).copied().unwrap_or(0)
    }

    /// Get raw opcode at PC.
    pub fn raw_opcode_at_pc(&self, pc: u64) -> u32 {
        self.pc_to_index(pc)
            .map(|idx| self.raw_opcode(idx))
            .unwrap_or(0)
    }

    /// Read a value from read-only memory.
    pub fn read_readonly(&self, addr: u64, size: usize) -> Option<u64> {
        for segment in &self.ro_segments {
            if let Some(value) = segment.read(addr, size) {
                return Some(value);
            }
        }
        None
    }

    /// Get PC of next instruction after given index.
    pub fn next_pc(&self, index: usize) -> u64 {
        let size = self.instruction_size(index);
        self.index_to_pc(index) + size as u64
    }

    /// Iterate over all valid instruction indices.
    pub fn valid_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.valid_mask
            .iter()
            .enumerate()
            .filter_map(|(i, &valid)| if valid { Some(i) } else { None })
    }

    /// Iterate over all valid instructions with their PCs.
    pub fn valid_instructions(&self) -> impl Iterator<Item = (u64, &DecodedInstr<X>)> + '_ {
        self.valid_indices().filter_map(move |idx| {
            self.get(idx).map(|instr| (self.index_to_pc(idx), instr))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::Rv64;

    #[test]
    fn test_instruction_table_basic() {
        let registry = ExtensionRegistry::<Rv64>::standard();
        // ADDI x1, x0, 42 (0x02a00093)
        let code = [0x93, 0x00, 0xa0, 0x02];
        let table = InstructionTable::from_bytes(&code, 0x80000000, &registry);

        assert_eq!(table.base_address(), 0x80000000);
        assert_eq!(table.end_address(), 0x80000004);
        assert_eq!(table.len(), 2); // 4 bytes / 2-byte slots

        assert!(table.is_valid_pc(0x80000000));
        assert!(!table.is_valid_pc(0x80000002)); // Second slot of 4-byte instruction

        let instr = table.get_at_pc(0x80000000).unwrap();
        assert_eq!(instr.size, 4);
    }

    #[test]
    fn test_instruction_table_compressed() {
        let registry = ExtensionRegistry::<Rv64>::standard();
        // C.ADDI x1, 1 (0x0085)
        let code = [0x85, 0x00];
        let table = InstructionTable::from_bytes(&code, 0x80000000, &registry);

        assert!(table.is_valid_pc(0x80000000));
        let instr = table.get_at_pc(0x80000000).unwrap();
        assert_eq!(instr.size, 2);
    }

    #[test]
    fn test_ro_segment_read() {
        let segment = RoSegment::new(0x1000, 0x1010, vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);

        assert_eq!(segment.read(0x1000, 1), Some(0x11));
        assert_eq!(segment.read(0x1000, 2), Some(0x2211));
        assert_eq!(segment.read(0x1000, 4), Some(0x44332211));
        assert_eq!(segment.read(0x1004, 4), Some(0x88776655));
        assert_eq!(segment.read(0x1008, 4), None); // Out of bounds
    }

    #[test]
    fn test_pc_to_index() {
        let table = InstructionTable::<Rv64>::new(0x80000000, 0x80001000, 0x80000000);

        assert_eq!(table.pc_to_index(0x80000000), Some(0));
        assert_eq!(table.pc_to_index(0x80000002), Some(1));
        assert_eq!(table.pc_to_index(0x80000004), Some(2));
        assert_eq!(table.pc_to_index(0x80000001), None); // Not aligned
        assert_eq!(table.pc_to_index(0x70000000), None); // Out of range
    }
}
