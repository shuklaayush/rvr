//! Emit configuration.

use std::collections::HashSet;
use std::marker::PhantomData;

use rvr_isa::{Xlen, NUM_REGS_I, NUM_REGS_E};

/// Instruction retirement counting mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstretMode {
    /// No instruction counting.
    Off,
    /// Count instructions but don't suspend.
    Count,
    /// Count instructions and suspend at limit.
    Suspend,
}

impl InstretMode {
    pub fn counts(&self) -> bool {
        *self != Self::Off
    }

    pub fn suspends(&self) -> bool {
        *self == Self::Suspend
    }
}

impl Default for InstretMode {
    fn default() -> Self {
        Self::Count
    }
}

/// Code generation configuration.
#[derive(Clone, Debug)]
pub struct EmitConfig<X: Xlen> {
    /// Number of registers: 32 for I extension, 16 for E extension.
    pub num_regs: usize,
    /// Registers passed as arguments (hot registers).
    pub hot_regs: Vec<u8>,
    /// Enable address bounds checking.
    pub addr_check: bool,
    /// Instruction retirement mode.
    pub instret_mode: InstretMode,
    /// Maximum instructions to retire before suspend (if Some).
    pub max_instret: Option<u64>,
    /// Emit comments in generated C code.
    pub emit_comments: bool,
    /// Emit line information (#line directives).
    pub emit_line_info: bool,
    /// Enable tohost check (for riscv-tests).
    pub tohost_enabled: bool,
    /// Memory address bits (default 32).
    pub memory_bits: u8,
    /// Valid instruction addresses.
    pub valid_addresses: HashSet<u64>,
    /// Program entry point.
    pub entry_point: X::Reg,
    /// Program end address.
    pub pc_end: X::Reg,
    /// Tracer configuration.
    pub tracer_config: TracerConfig,
    _marker: PhantomData<X>,
}

impl<X: Xlen> Default for EmitConfig<X> {
    fn default() -> Self {
        Self {
            num_regs: NUM_REGS_I,
            hot_regs: Vec::new(),
            addr_check: false,
            instret_mode: InstretMode::Count,
            max_instret: None,
            emit_comments: true,
            emit_line_info: false,
            tohost_enabled: false,
            memory_bits: 32,
            valid_addresses: HashSet::new(),
            entry_point: X::from_u64(0),
            pc_end: X::from_u64(0),
            tracer_config: TracerConfig::none(),
            _marker: PhantomData,
        }
    }
}

impl<X: Xlen> EmitConfig<X> {
    /// Create config with default settings.
    pub fn new(num_regs: usize) -> Self {
        assert!(num_regs == NUM_REGS_I || num_regs == NUM_REGS_E);
        Self {
            num_regs,
            ..Default::default()
        }
    }

    /// Check if register index is valid.
    pub fn is_valid_reg(&self, reg: u8) -> bool {
        (reg as usize) < self.num_regs
    }

    /// Check if register is in hot list.
    pub fn is_hot_reg(&self, reg: u8) -> bool {
        reg != 0 && self.hot_regs.contains(&reg)
    }

    /// Set address checking.
    pub fn with_addr_check(mut self, enabled: bool) -> Self {
        self.addr_check = enabled;
        self
    }

    /// Set tracer configuration.
    pub fn with_tracer(mut self, config: TracerConfig) -> Self {
        self.tracer_config = config;
        self
    }
}

use crate::tracer::TracerConfig;
