//! Derived inputs for emission (computed from the ELF/CFG pipeline).

use std::collections::{HashMap, HashSet};

/// Inputs derived from the program/CFG, not from user configuration.
#[derive(Clone, Debug, Default)]
pub struct EmitInputs {
    /// Entry point address (where execution starts).
    pub entry_point: u64,
    /// Start of text section (lowest code address, used for dispatch table).
    pub text_start: u64,
    /// End address (exclusive).
    pub pc_end: u64,
    /// Valid block start addresses.
    pub valid_addresses: HashSet<u64>,
    /// Absorbed block mapping: absorbed_pc -> merged_block_start.
    pub absorbed_to_merged: HashMap<u64, u64>,
    /// Initial brk value (end of bss section).
    pub initial_brk: u64,
}

impl EmitInputs {
    /// Create inputs with entry point and end address.
    /// By default, text_start is set to entry_point.
    pub fn new(entry_point: u64, pc_end: u64) -> Self {
        Self {
            entry_point,
            text_start: entry_point,
            pc_end,
            valid_addresses: HashSet::new(),
            absorbed_to_merged: HashMap::new(),
            initial_brk: 0,
        }
    }

    /// Set the text start address (lowest code address).
    pub fn with_text_start(mut self, text_start: u64) -> Self {
        self.text_start = text_start;
        self
    }

    /// Set the initial brk value.
    pub fn with_initial_brk(mut self, brk: u64) -> Self {
        self.initial_brk = brk;
        self
    }

    /// Check if address is valid (either directly or via absorbed mapping).
    pub fn is_valid_address(&self, pc: u64) -> bool {
        self.valid_addresses.contains(&pc) || self.absorbed_to_merged.contains_key(&pc)
    }

    /// Resolve an address to its actual target (handles absorbed blocks).
    pub fn resolve_address(&self, pc: u64) -> u64 {
        self.absorbed_to_merged.get(&pc).copied().unwrap_or(pc)
    }
}
