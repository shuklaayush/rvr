//! Core emission helpers for ARM64 assembly.
//!
//! Low-level text emission, register helpers, and address translation.

use rvr_ir::Xlen;

use super::Arm64Emitter;
use super::registers::reserved;

impl<X: Xlen> Arm64Emitter<X> {
    /// Number of temp slots for IR Temp values.
    pub const TEMP_SLOTS: usize = 8;
    /// Bytes per temp slot (use 8 for alignment even in RV32).
    pub const TEMP_SLOT_BYTES: usize = 8;
    /// Total temp stack bytes.
    pub const TEMP_STACK_BYTES: usize = Self::TEMP_SLOTS * Self::TEMP_SLOT_BYTES;

    /// Generate a unique label.
    pub(super) fn next_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!(".L{}_{}", prefix, self.label_counter)
    }

    // ========================================================================
    // Low-level emission helpers
    // ========================================================================

    /// Emit an assembly line with indentation.
    pub(super) fn emit(&mut self, line: &str) {
        self.asm.push_str("    ");
        self.asm.push_str(line);
        self.asm.push('\n');
    }

    /// Emit a formatted assembly line.
    pub(super) fn emitf(&mut self, line: impl std::fmt::Display) {
        self.asm.push_str("    ");
        self.asm.push_str(&line.to_string());
        self.asm.push('\n');
    }

    /// Emit a label.
    pub(super) fn emit_label(&mut self, name: &str) {
        self.asm.push_str(name);
        self.asm.push_str(":\n");
    }

    /// Emit a PC label.
    pub(super) fn emit_pc_label(&mut self, pc: u64) {
        self.asm.push_str(&format!("asm_pc_{:x}:\n", pc));
    }

    /// Emit a raw line (no indentation).
    pub(super) fn emit_raw(&mut self, line: &str) {
        self.asm.push_str(line);
        self.asm.push('\n');
    }

    /// Emit a comment.
    pub(super) fn emit_comment(&mut self, comment: &str) {
        self.asm.push_str("    // ");
        self.asm.push_str(comment);
        self.asm.push('\n');
    }

    /// Emit an empty line.
    pub(super) fn emit_blank(&mut self) {
        self.asm.push('\n');
    }

    // ========================================================================
    // Register size helpers
    // ========================================================================

    /// Get the appropriate temp register name for current XLEN.
    /// For RV32: w0, w1, w2. For RV64: x0, x1, x2.
    pub(super) fn temp1(&self) -> &'static str {
        if X::VALUE == 32 { "w0" } else { "x0" }
    }

    pub(super) fn temp2(&self) -> &'static str {
        if X::VALUE == 32 { "w1" } else { "x1" }
    }

    #[allow(dead_code)]
    pub(super) fn temp3(&self) -> &'static str {
        if X::VALUE == 32 { "w2" } else { "x2" }
    }

    /// Get stack offset for a temp slot (relative to current SP).
    pub(super) fn temp_slot_offset(&self, idx: u8) -> Option<usize> {
        let idx = idx as usize;
        if idx < Self::TEMP_SLOTS {
            Some(idx * Self::TEMP_SLOT_BYTES)
        } else {
            None
        }
    }

    /// Allocate a spill slot (starting at slot 4) for nested binary ops.
    pub(super) fn alloc_spill_slot(&mut self) -> Option<usize> {
        let idx = 4 + self.spill_depth;
        if idx < Self::TEMP_SLOTS {
            self.spill_depth += 1;
            Some(idx * Self::TEMP_SLOT_BYTES)
        } else {
            None
        }
    }

    /// Release the most recently allocated spill slot.
    pub(super) fn release_spill_slot(&mut self) {
        if self.spill_depth > 0 {
            self.spill_depth -= 1;
        }
    }

    /// Get the 32-bit version of any ARM64 register.
    pub(super) fn reg_32(&self, reg: &str) -> String {
        if let Some(suffix) = reg.strip_prefix('x') {
            format!("w{suffix}")
        } else {
            reg.to_string()
        }
    }

    /// Get the 64-bit version of any ARM64 register.
    pub(super) fn reg_64(&self, reg: &str) -> String {
        if let Some(suffix) = reg.strip_prefix('w') {
            format!("x{suffix}")
        } else {
            reg.to_string()
        }
    }

    // ========================================================================
    // Register access helpers
    // ========================================================================

    /// Get the ARM64 register for a RISC-V register (hot regs only).
    /// Returns None for cold registers.
    #[allow(dead_code)]
    pub(super) fn rv_reg(&self, reg: u8) -> Option<&'static str> {
        if reg == 0 {
            return None;
        }
        self.reg_map.get(reg)
    }

    /// Load a RISC-V register value into a temporary ARM64 register.
    /// Returns the register name to use.
    pub(super) fn load_rv_to_temp(&mut self, rv_reg: u8, temp: &str) -> String {
        if rv_reg == 0 {
            // Zero register - just move zero
            self.emitf(format!("mov {temp}, #0"));
            return temp.to_string();
        }
        if let Some(arm_reg) = self.reg_map.get(rv_reg) {
            // Already in a register
            arm_reg.to_string()
        } else {
            // Load from memory
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "ldr {temp}, [{}, #{}]",
                reserved::STATE_PTR,
                offset
            ));
            temp.to_string()
        }
    }

    /// Load a RISC-V register value as a 64-bit address.
    /// For RV32, zero-extends to 64-bit. For RV64, loads directly.
    /// Always returns a 64-bit register name.
    pub(super) fn load_rv_as_addr(&mut self, rv_reg: u8, temp64: &str) -> String {
        if rv_reg == 0 {
            self.emitf(format!("mov {temp64}, #0"));
            return temp64.to_string();
        }
        if let Some(arm_reg) = self.reg_map.get(rv_reg) {
            if X::VALUE == 32 {
                // RV32: hot regs are 32-bit, zero-extend to 64-bit
                let reg64 = self.reg_64(arm_reg);
                // Use uxtw to zero-extend
                self.emitf(format!("mov {temp64}, {reg64}"));
                temp64.to_string()
            } else {
                arm_reg.to_string()
            }
        } else {
            let offset = self.layout.reg_offset(rv_reg);
            if X::VALUE == 32 {
                // Load 32-bit, zero-extends to 64-bit via ldr w -> x
                let temp32 = self.reg_32(temp64);
                self.emitf(format!(
                    "ldr {temp32}, [{}, #{}]",
                    reserved::STATE_PTR,
                    offset
                ));
            } else {
                self.emitf(format!(
                    "ldr {temp64}, [{}, #{}]",
                    reserved::STATE_PTR,
                    offset
                ));
            }
            temp64.to_string()
        }
    }

    /// Store a value to a RISC-V register.
    pub(super) fn store_to_rv(&mut self, rv_reg: u8, value: &str) {
        if rv_reg == 0 {
            return; // x0 ignores writes
        }
        if let Some(arm_reg) = self.reg_map.get(rv_reg) {
            if arm_reg != value {
                self.emitf(format!("mov {arm_reg}, {value}"));
            }
        } else {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "str {value}, [{}, #{}]",
                reserved::STATE_PTR,
                offset
            ));
        }
    }

    // ========================================================================
    // Immediate helpers
    // ========================================================================

    /// Load an immediate value into a register.
    /// ARM64 has limited immediate encoding, so large values need movz/movk sequences.
    pub(super) fn load_imm(&mut self, dest: &str, value: u64) {
        if value == 0 {
            self.emitf(format!("mov {dest}, #0"));
            return;
        }

        let is_32bit = dest.starts_with('w');
        let val = if is_32bit { value as u32 as u64 } else { value };

        // Check if it fits in a single mov with shifted immediate
        // ARM64 can move 16-bit immediate with optional shift
        if val <= 0xFFFF {
            self.emitf(format!("mov {dest}, #{val}"));
            return;
        }

        // Check if it can be encoded as a bitmask immediate
        // (this is complex, skip for now and use movz/movk)

        // For 32-bit destination
        if is_32bit {
            let v = val as u32;
            self.emitf(format!("movz {dest}, #{}", v & 0xFFFF));
            if (v >> 16) != 0 {
                self.emitf(format!("movk {dest}, #{}, lsl #16", (v >> 16) & 0xFFFF));
            }
            return;
        }

        // For 64-bit destination, may need up to 4 instructions
        let mut emitted = false;
        for shift in [0, 16, 32, 48] {
            let chunk = ((val >> shift) & 0xFFFF) as u16;
            if chunk != 0 {
                if !emitted {
                    self.emitf(format!("movz {dest}, #{chunk}, lsl #{shift}"));
                    emitted = true;
                } else {
                    self.emitf(format!("movk {dest}, #{chunk}, lsl #{shift}"));
                }
            }
        }

        // If value was 0 but we're here (shouldn't happen), just mov 0
        if !emitted {
            self.emitf(format!("mov {dest}, #0"));
        }
    }

    /// Load a signed immediate that might be negative.
    #[allow(dead_code)]
    pub(super) fn load_signed_imm(&mut self, dest: &str, value: i64) {
        if value >= 0 {
            self.load_imm(dest, value as u64);
        } else {
            // For negative values, load positive and negate, or use mvn for -1 patterns
            if value == -1 {
                self.emitf(format!("mov {dest}, #-1"));
            } else {
                self.load_imm(dest, value as u64);
            }
        }
    }

    /// Add an offset to a base register, handling negative offsets.
    /// ARM64 add immediate doesn't accept negative values, so we use sub for negative offsets.
    pub(super) fn emit_add_offset(&mut self, dest: &str, base: &str, offset: i64) {
        if offset == 0 {
            if dest != base {
                self.emitf(format!("mov {dest}, {base}"));
            }
        } else if (1..=4095).contains(&offset) {
            // Fits in 12-bit unsigned immediate
            self.emitf(format!("add {dest}, {base}, #{offset}"));
        } else if (-4095..0).contains(&offset) {
            // Negative offset that fits in 12-bit - use sub
            self.emitf(format!("sub {dest}, {base}, #{}", -offset));
        } else {
            // Large offset - load into temp and add
            self.load_imm("x2", offset as u64);
            self.emitf(format!("add {dest}, {base}, x2"));
        }
    }

    // ========================================================================
    // Address translation
    // ========================================================================

    /// Apply address translation to an address in a register.
    ///
    /// Uses AddressMode semantics:
    /// - Unchecked: no-op (guard pages catch OOB)
    /// - Wrap: mask address to memory size
    /// - Bounds: check bounds, trap if OOB, then mask
    pub(super) fn apply_address_mode(&mut self, addr_reg: &str) {
        let mode = self.config.address_mode;

        // Bounds check (Bounds mode only)
        if mode.needs_bounds_check() {
            let ok_label = self.next_label("bounds_ok");
            let addr64 = self.reg_64(addr_reg);

            // Check if address is within bounds
            // Valid addresses have high bits either all 0 or all 1 (for sign-extended negatives)
            // Check: (addr >> memory_bits) == 0 || (addr >> memory_bits) == -1
            let shift_amount = self.config.memory_bits;
            self.emitf(format!("asr x2, {addr64}, #{shift_amount}")); // arithmetic shift for sign
            self.emitf(format!("cbz x2, {ok_label}")); // all zeros is valid
            self.emit("cmn x2, #1"); // compare with -1 (all ones)
            self.emitf(format!("b.eq {ok_label}")); // all ones is valid
            self.emit("b asm_trap");
            self.emit_label(&ok_label);
        }

        // Address masking (Wrap and Bounds modes)
        if mode.needs_mask() {
            let mask = self.memory_mask;
            let addr64 = self.reg_64(addr_reg);

            // Load mask and AND
            if mask <= 0xFFF {
                // 12-bit immediate fits in and instruction
                self.emitf(format!("and {addr64}, {addr64}, #{mask}"));
            } else {
                self.load_imm("x2", mask);
                self.emitf(format!("and {addr64}, {addr64}, x2"));
            }
        }
    }

    // ========================================================================
    // Instret handling
    // ========================================================================

    /// Emit instret increment for an instruction.
    pub(super) fn emit_instret_increment(&mut self, count: u64, pc: u64) {
        if !self.config.instret_mode.counts() {
            return;
        }

        // Increment instret counter (always 64-bit)
        let instret_offset = self.layout.offset_instret;
        self.emitf(format!(
            "ldr x2, [{}, #{}]",
            reserved::STATE_PTR,
            instret_offset
        ));
        self.emitf(format!("add x2, x2, #{count}"));
        self.emitf(format!(
            "str x2, [{}, #{}]",
            reserved::STATE_PTR,
            instret_offset
        ));

        // For suspend mode, check if we hit the limit
        if self.config.instret_mode.suspends() {
            let continue_label = self.next_label("instret_ok");
            let target_offset = self.layout.offset_target_instret;

            self.emitf(format!(
                "ldr x1, [{}, #{}]",
                reserved::STATE_PTR,
                target_offset
            ));
            self.emit("cmp x2, x1");
            self.emitf(format!("b.lo {continue_label}"));

            // Suspend: save current PC
            let pc_offset = self.layout.offset_pc;
            if X::VALUE == 32 {
                self.load_imm("w1", pc);
                self.emitf(format!("str w1, [{}, #{}]", reserved::STATE_PTR, pc_offset));
            } else {
                self.load_imm("x1", pc);
                self.emitf(format!("str x1, [{}, #{}]", reserved::STATE_PTR, pc_offset));
            }
            self.emit("b asm_exit");
            self.emit_label(&continue_label);
        }
    }
}
