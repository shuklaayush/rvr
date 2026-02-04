//! `RvState` layout computation.
//!
//! Computes field offsets for the `RvState` struct to match the C header.
//! This is the single source of truth for layout, used by both C and x86 emitters.

use rvr_ir::Xlen;

use crate::config::EmitConfig;

/// `RvState` field offsets.
///
/// All offsets are in bytes from the start of the struct.
#[derive(Clone, Debug)]
pub struct RvStateLayout {
    /// Bytes per register (4 for RV32, 8 for RV64).
    pub reg_bytes: usize,
    /// Number of registers (32 for I, 16 for E).
    pub num_regs: usize,
    /// Offset of regs[0].
    pub offset_regs: usize,
    /// Offset of pc.
    pub offset_pc: usize,
    /// Offset of instret.
    pub offset_instret: usize,
    /// Offset of `target_instret` (only valid if `instret_suspend` is true).
    pub offset_target_instret: usize,
    /// Whether instret suspend mode is enabled.
    pub instret_suspend: bool,
    /// Offset of `reservation_addr`.
    pub offset_reservation_addr: usize,
    /// Offset of `reservation_valid`.
    pub offset_reservation_valid: usize,
    /// Offset of `has_exited`.
    pub offset_has_exited: usize,
    /// Offset of `exit_code`.
    pub offset_exit_code: usize,
    /// Offset of brk.
    pub offset_brk: usize,
    /// Offset of `start_brk`.
    pub offset_start_brk: usize,
    /// Offset of memory pointer.
    pub offset_memory: usize,
    /// Offset of tracer field (immediately after memory pointer).
    pub offset_tracer: usize,
}

impl RvStateLayout {
    /// Compute layout from emit config.
    #[must_use]
    pub const fn new<X: Xlen>(config: &EmitConfig<X>) -> Self {
        Self::from_params(
            X::REG_BYTES,
            config.num_regs,
            config.instret_mode.suspends(),
        )
    }

    /// Compute layout from raw parameters.
    ///
    /// This is the core implementation used by both `new` and direct callers
    /// (like the C header generator) that don't have a full `EmitConfig`.
    #[must_use]
    pub const fn from_params(reg_bytes: usize, num_regs: usize, has_suspend: bool) -> Self {
        // Hot fields first for cache locality
        let offset_regs = 0;
        let size_regs = num_regs * reg_bytes;
        let offset_pc = offset_regs + size_regs;

        // instret is uint64_t, needs 8-byte alignment
        let instret_unaligned = offset_pc + reg_bytes;
        let offset_instret = (instret_unaligned + 7) & !7;

        // Optional target_instret for suspend mode
        let offset_target_instret = offset_instret + 8;
        let suspender_size = if has_suspend { 8 } else { 0 };

        // Reservation for LR/SC
        let offset_reservation_addr = offset_instret + 8 + suspender_size;
        let offset_reservation_valid = offset_reservation_addr + reg_bytes;

        // Execution control (packed booleans)
        let offset_has_exited = offset_reservation_valid + 1;
        let offset_exit_code = offset_has_exited + 1;
        let offset_pad0 = offset_exit_code + 1;

        // Align to 8 bytes for brk
        let brk_align_offset = offset_pad0 + 1;
        let brk_padding = (8 - (brk_align_offset % 8)) % 8;
        let offset_brk = brk_align_offset + brk_padding;
        let offset_start_brk = offset_brk + reg_bytes;

        // Memory pointer
        let offset_memory = offset_start_brk + reg_bytes;
        let offset_tracer = offset_memory + 8;

        Self {
            reg_bytes,
            num_regs,
            offset_regs,
            offset_pc,
            offset_instret,
            offset_target_instret,
            instret_suspend: has_suspend,
            offset_reservation_addr,
            offset_reservation_valid,
            offset_has_exited,
            offset_exit_code,
            offset_brk,
            offset_start_brk,
            offset_memory,
            offset_tracer,
        }
    }

    /// Get offset of a specific register.
    #[must_use]
    pub const fn reg_offset(&self, reg: u8) -> usize {
        self.offset_regs + (reg as usize) * self.reg_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::{Rv32, Rv64};

    #[test]
    fn test_rv64_layout() {
        let config = EmitConfig::<Rv64>::default();
        let layout = RvStateLayout::new::<Rv64>(&config);

        assert_eq!(layout.reg_bytes, 8);
        assert_eq!(layout.num_regs, 32);
        assert_eq!(layout.offset_regs, 0);
        assert_eq!(layout.offset_pc, 32 * 8); // 256
        // After pc (8 bytes), instret should be at 264 (already aligned)
        assert_eq!(layout.offset_instret, 264);
    }

    #[test]
    fn test_rv32_layout() {
        let config = EmitConfig::<Rv32>::default();
        let layout = RvStateLayout::new::<Rv32>(&config);

        assert_eq!(layout.reg_bytes, 4);
        assert_eq!(layout.num_regs, 32);
        assert_eq!(layout.offset_regs, 0);
        assert_eq!(layout.offset_pc, 32 * 4); // 128
        // After pc (4 bytes), instret needs 8-byte alignment
        // 128 + 4 = 132, align to 8 -> 136
        assert_eq!(layout.offset_instret, 136);
    }

    #[test]
    fn test_reg_offset() {
        let config = EmitConfig::<Rv64>::default();
        let layout = RvStateLayout::new::<Rv64>(&config);

        assert_eq!(layout.reg_offset(0), 0);
        assert_eq!(layout.reg_offset(1), 8);
        assert_eq!(layout.reg_offset(10), 80);
        assert_eq!(layout.reg_offset(31), 248);
    }
}
