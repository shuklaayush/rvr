//! Instruction table for decoded instructions.
//!
//! Maintains decoded instructions, sizes, and raw opcodes.
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

/// Slot for a single 2-byte instruction position.
#[derive(Clone, Debug)]
struct Slot<X: Xlen> {
    instr: Option<DecodedInstr<X>>,
    size: u8,
    raw: u32,
}

impl<X: Xlen> Default for Slot<X> {
    fn default() -> Self {
        Self {
            instr: None,
            size: 0,
            raw: 0,
        }
    }
}

/// Table of decoded instructions.
///
/// Maintains instruction slots indexed by PC, with 2-byte slot size.
/// Handles both compressed (2-byte) and full (4-byte) instructions.
pub struct InstructionTable<X: Xlen> {
    /// Decoded instruction slots (indexed by slot).
    slots: Vec<Slot<X>>,
    /// Base address of the table.
    base_address: u64,
    /// End address (exclusive).
    end_address: u64,
    /// Entry point address.
    entry_point: u64,
    /// Read-only segments for constant propagation.
    ro_segments: Vec<RoSegment>,
}

impl<X: Xlen> InstructionTable<X> {
    /// Slot size in bytes (2 for RISC-V with C extension support).
    pub const SLOT_SIZE: usize = 2;

    /// Create a new instruction table from raw bytes.
    pub fn from_bytes(code: &[u8], base_address: u64, registry: &ExtensionRegistry<X>) -> Self {
        let end_address = base_address + code.len() as u64;
        let total_slots = code.len().div_ceil(Self::SLOT_SIZE);

        let mut table = Self {
            slots: vec![Slot::default(); total_slots],
            base_address,
            end_address,
            entry_point: base_address,
            ro_segments: vec![RoSegment::new(base_address, end_address, code.to_vec())],
        };

        table.decode_all(code, 0, registry);
        table
    }

    /// Create a new instruction table with specific address range.
    pub fn new(base_address: u64, end_address: u64, entry_point: u64) -> Self {
        let total_size = (end_address - base_address) as usize;
        let total_slots = total_size.div_ceil(Self::SLOT_SIZE);

        Self {
            slots: vec![Slot::default(); total_slots],
            base_address,
            end_address,
            entry_point,
            ro_segments: Vec::new(),
        }
    }

    /// Decode all instructions from code at given slot offset.
    fn decode_all(&mut self, code: &[u8], start_slot: usize, registry: &ExtensionRegistry<X>) {
        let mut offset = 0;

        while offset + 2 <= code.len() {
            let pc = self.base_address + (start_slot * Self::SLOT_SIZE + offset) as u64;
            let slot = start_slot + offset / Self::SLOT_SIZE;

            if slot >= self.slots.len() {
                break;
            }

            if let Some(instr) = registry.decode(&code[offset..], X::from_u64(pc)) {
                let size = instr.size as usize;
                let raw = if size == 2 {
                    u16::from_le_bytes([code[offset], code[offset + 1]]) as u32
                } else if size == 4 && offset + 4 <= code.len() {
                    u32::from_le_bytes([
                        code[offset],
                        code[offset + 1],
                        code[offset + 2],
                        code[offset + 3],
                    ])
                } else {
                    0
                };

                self.slots[slot] = Slot {
                    instr: Some(instr),
                    size: size as u8,
                    raw,
                };

                if size == 4 && slot + 1 < self.slots.len() {
                    self.slots[slot + 1] = Slot::default();
                }

                offset += size;
            } else {
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

        let start_slot = ((segment_start - self.base_address) / Self::SLOT_SIZE as u64) as usize;
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
            let slot = start_slot + offset / Self::SLOT_SIZE;

            if slot >= self.slots.len() {
                break;
            }

            if let Some(instr) = registry.decode(&code[offset..], X::from_u64(pc)) {
                let size = instr.size as usize;
                let raw = if size == 2 {
                    u16::from_le_bytes([code[offset], code[offset + 1]]) as u32
                } else if size == 4 && offset + 4 <= code.len() {
                    u32::from_le_bytes([
                        code[offset],
                        code[offset + 1],
                        code[offset + 2],
                        code[offset + 3],
                    ])
                } else {
                    0
                };

                self.slots[slot] = Slot {
                    instr: Some(instr),
                    size: size as u8,
                    raw,
                };

                if size == 4 && slot + 1 < self.slots.len() {
                    self.slots[slot + 1] = Slot::default();
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

    /// Get read-only segments for scanning.
    pub fn ro_segments(&self) -> &[RoSegment] {
        &self.ro_segments
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
        self.slots.len()
    }

    /// Check if table is empty.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Convert PC to slot index.
    pub fn pc_to_index(&self, pc: u64) -> Option<usize> {
        if pc < self.base_address || pc >= self.end_address {
            return None;
        }
        let offset = (pc - self.base_address) as usize;
        if !offset.is_multiple_of(Self::SLOT_SIZE) {
            return None;
        }
        Some(offset / Self::SLOT_SIZE)
    }

    /// Convert slot index to PC.
    pub fn index_to_pc(&self, index: usize) -> u64 {
        self.base_address + (index * Self::SLOT_SIZE) as u64
    }

    /// Check if slot is valid.
    pub fn is_valid_index(&self, index: usize) -> bool {
        self.slots
            .get(index)
            .map(|slot| slot.instr.is_some())
            .unwrap_or(false)
    }

    /// Check if PC points to a valid instruction.
    pub fn is_valid_pc(&self, pc: u64) -> bool {
        self.pc_to_index(pc)
            .map(|idx| self.is_valid_index(idx))
            .unwrap_or(false)
    }

    /// Get instruction at slot index.
    pub fn get(&self, index: usize) -> Option<&DecodedInstr<X>> {
        self.slots.get(index).and_then(|slot| slot.instr.as_ref())
    }

    /// Get instruction at PC.
    pub fn get_at_pc(&self, pc: u64) -> Option<&DecodedInstr<X>> {
        self.pc_to_index(pc).and_then(|idx| self.get(idx))
    }

    /// Get instruction size at slot index.
    pub fn instruction_size(&self, index: usize) -> u8 {
        self.slots.get(index).map(|slot| slot.size).unwrap_or(0)
    }

    /// Get instruction size at PC.
    pub fn instruction_size_at_pc(&self, pc: u64) -> u8 {
        self.pc_to_index(pc)
            .map(|idx| self.instruction_size(idx))
            .unwrap_or(0)
    }

    /// Get raw opcode at slot index.
    pub fn raw_opcode(&self, index: usize) -> u32 {
        self.slots.get(index).map(|slot| slot.raw).unwrap_or(0)
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
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| if slot.instr.is_some() { Some(i) } else { None })
    }

    /// Iterate over all valid instructions with their PCs.
    pub fn valid_instructions(&self) -> impl Iterator<Item = (u64, &DecodedInstr<X>)> + '_ {
        self.valid_indices()
            .filter_map(move |idx| self.get(idx).map(|instr| (self.index_to_pc(idx), instr)))
    }
}
