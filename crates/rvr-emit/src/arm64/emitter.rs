//! Core emission helpers for ARM64 assembly.
//!
//! Low-level text emission, register helpers, and address translation.

use rvr_ir::Xlen;

use crate::c::TracerKind;

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
    /// Resets the cold cache since labels are entry points where
    /// the cold cache register (x17) might contain stale data.
    pub(super) fn emit_pc_label(&mut self, pc: u64) {
        self.cold_cache = None;
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

    /// Save all hot registers to state (before external calls).
    pub(super) fn save_hot_regs_to_state(&mut self) {
        let hot_regs: Vec<_> = self.reg_map.hot_regs().collect();
        for (rv_reg, arm_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "str {arm_reg}, [{}, #{}]",
                reserved::STATE_PTR,
                offset
            ));
        }
        if self.config.instret_mode.counts() {
            let instret_off = self.layout.offset_instret;
            self.emitf(format!(
                "str {}, [{}, #{}]",
                reserved::INSTRET,
                reserved::STATE_PTR,
                instret_off
            ));
        }
        self.cold_cache = None;
    }

    /// Restore all hot registers from state (after external calls).
    pub(super) fn restore_hot_regs_from_state(&mut self) {
        let hot_regs: Vec<_> = self.reg_map.hot_regs().collect();
        for (rv_reg, arm_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "ldr {arm_reg}, [{}, #{}]",
                reserved::STATE_PTR,
                offset
            ));
        }
        if self.config.instret_mode.counts() {
            let instret_off = self.layout.offset_instret;
            self.emitf(format!(
                "ldr {}, [{}, #{}]",
                reserved::INSTRET,
                reserved::STATE_PTR,
                instret_off
            ));
        }
        self.cold_cache = None;
    }

    pub(super) fn cold_cache_reg(&self) -> &'static str {
        if X::VALUE == 32 {
            "w17"
        } else {
            reserved::COLD_CACHE
        }
    }

    pub(super) fn cold_cache_hit(&self, rv_reg: u8) -> Option<&'static str> {
        if self.cold_cache == Some(rv_reg) {
            Some(self.cold_cache_reg())
        } else {
            None
        }
    }

    pub(super) fn cold_cache_set(&mut self, rv_reg: u8) -> &'static str {
        self.cold_cache = Some(rv_reg);
        self.cold_cache_reg()
    }

    pub(super) fn cold_cache_invalidate(&mut self, rv_reg: u8) {
        if self.cold_cache == Some(rv_reg) {
            self.cold_cache = None;
        }
    }

    // ========================================================================
    // Diff tracer helpers (ASM backends)
    // ========================================================================

    fn tracer_kind(&self) -> Option<TracerKind> {
        self.config.tracer_config.builtin_kind()
    }

    fn diff_tracer_enabled(&self) -> bool {
        matches!(self.tracer_kind(), Some(TracerKind::Diff))
    }

    fn diff_offsets(
        &self,
    ) -> (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    ) {
        let reg_bytes = X::REG_BYTES;
        let off_opcode = reg_bytes;
        let off_rd = reg_bytes + 4;
        let off_rd_value = reg_bytes + 8;
        let off_mem_addr = off_rd_value + reg_bytes;
        let off_mem_value = off_mem_addr + reg_bytes;
        let off_mem_width = off_mem_value + reg_bytes;
        let off_is_write = off_mem_width + 1;
        let off_has_rd = off_is_write + 1;
        let off_has_mem = off_has_rd + 1;
        let off_valid = off_has_mem + 1;
        (
            off_opcode,
            off_rd,
            off_rd_value,
            off_mem_addr,
            off_mem_value,
            off_mem_width,
            off_is_write,
            off_has_rd,
            off_has_mem,
            off_valid,
        )
    }

    pub(super) fn emit_trace_pc(&mut self, pc: u64, opcode: u32) {
        if !self.diff_tracer_enabled() {
            return;
        }

        let tracer_base = self.layout.offset_tracer;
        let (
            off_opcode,
            off_rd,
            off_rd_value,
            off_mem_addr,
            off_mem_value,
            off_mem_width,
            off_is_write,
            off_has_rd,
            off_has_mem,
            off_valid,
        ) = self.diff_offsets();

        let tmp = self.temp1();
        let tmp32 = self.reg_32(tmp);

        // pc
        self.load_imm(tmp, pc);
        if X::VALUE == 32 {
            self.emitf(format!(
                "str {tmp32}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base
            ));
        } else {
            self.emitf(format!(
                "str {tmp}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base
            ));
        }

        // opcode
        self.load_imm(tmp, opcode as u64);
        self.emitf(format!(
            "str {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_opcode
        ));

        // Clear fields
        self.emitf(format!("mov {tmp32}, #0"));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_rd
        ));
        if X::VALUE == 32 {
            self.emitf(format!(
                "str {tmp32}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_rd_value
            ));
            self.emitf(format!(
                "str {tmp32}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_addr
            ));
            self.emitf(format!(
                "str {tmp32}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_value
            ));
        } else {
            self.emitf(format!(
                "str xzr, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_rd_value
            ));
            self.emitf(format!(
                "str xzr, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_addr
            ));
            self.emitf(format!(
                "str xzr, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_value
            ));
        }
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_mem_width
        ));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_is_write
        ));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_has_rd
        ));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_has_mem
        ));
        self.emitf(format!("mov {tmp32}, #1"));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_valid
        ));
    }

    pub(super) fn emit_trace_reg_write(&mut self, reg: u8, val_reg: &str) {
        if !self.diff_tracer_enabled() || reg == 0 {
            return;
        }

        let tracer_base = self.layout.offset_tracer;
        let (
            _off_opcode,
            off_rd,
            off_rd_value,
            _off_mem_addr,
            _off_mem_value,
            _off_mem_width,
            _off_is_write,
            off_has_rd,
            _off_has_mem,
            _off_valid,
        ) = self.diff_offsets();

        let tmp = self.temp3();
        let tmp32 = self.reg_32(tmp);
        self.emitf(format!("mov {tmp32}, #{reg}"));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_rd
        ));

        if X::VALUE == 32 {
            let val32 = self.reg_32(val_reg);
            self.emitf(format!(
                "str {val32}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_rd_value
            ));
        } else {
            let val64 = self.reg_64(val_reg);
            self.emitf(format!(
                "str {val64}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_rd_value
            ));
        }

        self.emitf(format!("mov {tmp32}, #1"));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_has_rd
        ));
    }

    pub(super) fn emit_trace_mem_access(
        &mut self,
        addr_reg: &str,
        val_reg: &str,
        width: u8,
        is_write: bool,
    ) {
        if !self.diff_tracer_enabled() {
            return;
        }

        let tracer_base = self.layout.offset_tracer;
        let (_, _, _, off_mem_addr, off_mem_value, off_mem_width, off_is_write, _, off_has_mem, _) =
            self.diff_offsets();

        if X::VALUE == 32 {
            let addr32 = self.reg_32(addr_reg);
            self.emitf(format!(
                "str {addr32}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_addr
            ));
        } else {
            let addr64 = self.reg_64(addr_reg);
            self.emitf(format!(
                "str {addr64}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_addr
            ));
        }

        let tmp = self.temp3();
        let tmp32 = self.reg_32(tmp);
        match width {
            1 => self.emitf(format!("uxtb {tmp32}, {}", self.reg_32(val_reg))),
            2 => self.emitf(format!("uxth {tmp32}, {}", self.reg_32(val_reg))),
            4 => self.emitf(format!("mov {tmp32}, {}", self.reg_32(val_reg))),
            8 => {
                if X::VALUE == 32 {
                    self.emitf(format!("mov {tmp32}, {}", self.reg_32(val_reg)));
                } else {
                    self.emitf(format!("mov {tmp}, {}", self.reg_64(val_reg)));
                }
            }
            _ => self.emitf(format!("mov {tmp32}, {}", self.reg_32(val_reg))),
        }

        if X::VALUE == 32 {
            self.emitf(format!(
                "str {tmp32}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_value
            ));
        } else {
            let tmp64 = if width == 8 {
                tmp.to_string()
            } else {
                self.reg_64(tmp)
            };
            self.emitf(format!(
                "str {tmp64}, [{}, #{}]",
                reserved::STATE_PTR,
                tracer_base + off_mem_value
            ));
        }

        self.emitf(format!("mov {tmp32}, #{width}"));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_mem_width
        ));
        let write_flag = if is_write { 1 } else { 0 };
        self.emitf(format!("mov {tmp32}, #{write_flag}"));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_is_write
        ));
        self.emitf(format!("mov {tmp32}, #1"));
        self.emitf(format!(
            "strb {tmp32}, [{}, #{}]",
            reserved::STATE_PTR,
            tracer_base + off_has_mem
        ));
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
            if let Some(cached) = self.cold_cache_hit(rv_reg) {
                return cached.to_string();
            }
            // Load from memory
            let offset = self.layout.reg_offset(rv_reg);
            let cache_reg = self.cold_cache_set(rv_reg);
            self.emitf(format!(
                "ldr {cache_reg}, [{}, #{}]",
                reserved::STATE_PTR,
                offset
            ));
            cache_reg.to_string()
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

            if mask == 0xffff_ffff {
                let addr32 = self.reg_32(&addr64);
                self.emitf(format!("uxtw {addr64}, {addr32}"));
            } else if mask <= 0xFFF {
                // 12-bit immediate fits in and instruction
                self.emitf(format!("and {addr64}, {addr64}, #{mask}"));
            } else {
                self.load_imm("x2", mask);
                self.emitf(format!("and {addr64}, {addr64}, x2"));
            }
        }
    }

    /// Check if an immediate can be encoded as an ARM64 logical immediate.
    pub(super) fn is_logical_imm(&self, value: u64, is_32: bool) -> bool {
        let bits = if is_32 { 32 } else { 64 };
        let v = if is_32 { value as u32 as u64 } else { value };
        if v == 0 {
            return false;
        }
        let full_mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        if v == full_mask {
            return false;
        }

        let mut size = 2usize;
        while size <= bits {
            let mask = if size == 64 {
                u64::MAX
            } else {
                (1u64 << size) - 1
            };
            let pattern = v & mask;
            let mut replicated = 0u64;
            let mut shift = 0usize;
            while shift < bits {
                replicated |= pattern << shift;
                shift += size;
            }
            if replicated == v && Self::is_rotated_mask(pattern, size as u32) {
                return true;
            }
            size <<= 1;
        }
        false
    }

    fn is_rotated_mask(value: u64, size: u32) -> bool {
        if size == 0 || size > 64 {
            return false;
        }
        let mask = if size == 64 {
            u64::MAX
        } else {
            (1u64 << size) - 1
        };
        let v = value & mask;
        if v == 0 || v == mask {
            return false;
        }
        for rot in 0..size {
            let r = if rot == 0 {
                v
            } else {
                ((v >> rot) | (v << (size - rot))) & mask
            };
            if (r & (r + 1)) == 0 {
                return true;
            }
        }
        false
    }

    // ========================================================================
    // Instret handling
    // ========================================================================

    /// Emit instret increment for an instruction.
    pub(super) fn emit_instret_increment(&mut self, count: u64, pc: u64) {
        if !self.config.instret_mode.counts() {
            return;
        }

        // Increment instret counter (always 64-bit) in the cached register.
        if count <= 0xFFF {
            self.emitf(format!(
                "add {}, {}, #{count}",
                reserved::INSTRET,
                reserved::INSTRET
            ));
        } else {
            self.load_imm("x2", count);
            self.emitf(format!(
                "add {}, {}, x2",
                reserved::INSTRET,
                reserved::INSTRET
            ));
        }

        let _ = pc; // Instret suspend checks handled at block boundaries.
    }
}
