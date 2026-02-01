//! Core emission helpers for x86-64 assembly (AT&T syntax).
//!
//! Low-level text emission, register helpers, and address translation.

use rvr_ir::Xlen;

use super::X86Emitter;
use super::registers::reserved;

impl<X: Xlen> X86Emitter<X> {
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
        self.asm.push_str("    # ");
        self.asm.push_str(comment);
        self.asm.push('\n');
    }

    /// Emit an empty line.
    pub(super) fn emit_blank(&mut self) {
        self.asm.push('\n');
    }

    /// Save all hot registers to state (before external calls).
    pub(super) fn save_hot_regs_to_state(&mut self) {
        let suffix = self.suffix();
        let hot_regs: Vec<_> = self.reg_map.hot_regs().collect();
        for (rv_reg, x86_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "mov{suffix} %{x86_reg}, {offset}(%{})",
                reserved::STATE_PTR
            ));
        }
    }

    /// Restore all hot registers from state (after external calls).
    pub(super) fn restore_hot_regs_from_state(&mut self) {
        let suffix = self.suffix();
        let hot_regs: Vec<_> = self.reg_map.hot_regs().collect();
        for (rv_reg, x86_reg) in hot_regs {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "mov{suffix} {offset}(%{}), %{x86_reg}",
                reserved::STATE_PTR
            ));
        }
    }

    /// Get stack offset for a temp slot (relative to current %rsp).
    pub(super) fn temp_slot_offset(&self, idx: u8) -> Option<usize> {
        let idx = idx as usize;
        if idx < Self::TEMP_SLOTS {
            Some(idx * Self::TEMP_SLOT_BYTES)
        } else {
            None
        }
    }

    // ========================================================================
    // AT&T syntax helpers
    // ========================================================================

    /// Get instruction suffix for current XLEN.
    pub(super) fn suffix(&self) -> &'static str {
        if X::VALUE == 32 { "l" } else { "q" }
    }

    // ========================================================================
    // Register size helpers
    // ========================================================================

    /// Get the appropriate temp register name for current XLEN.
    /// For RV32: eax, ecx, edx. For RV64: rax, rcx, rdx.
    pub(super) fn temp1(&self) -> &'static str {
        if X::VALUE == 32 { "eax" } else { "rax" }
    }

    pub(super) fn temp2(&self) -> &'static str {
        if X::VALUE == 32 { "ecx" } else { "rcx" }
    }

    /// Third temp register (rdi/edi) - available after prologue.
    /// Use for parallel operations or when temp1/temp2 are busy.
    /// Particularly useful when temp2 (rcx) is needed for shift count.
    pub(super) fn temp3(&self) -> &'static str {
        if X::VALUE == 32 { "edi" } else { "rdi" }
    }

    /// Get the dword-sized version of a temp register.
    pub(super) fn temp_dword(&self, temp: &str) -> &'static str {
        match temp {
            "rax" | "eax" => "eax",
            "rcx" | "ecx" => "ecx",
            "rdx" | "edx" => "edx",
            _ => "eax",
        }
    }

    /// Get the qword-sized version of a temp register.
    pub(super) fn temp_qword(&self, temp: &str) -> &'static str {
        match temp {
            "rax" | "eax" => "rax",
            "rcx" | "ecx" => "rcx",
            "rdx" | "edx" => "rdx",
            _ => "rax",
        }
    }

    /// Get the byte-sized version of any x86 register.
    pub(super) fn reg_byte(&self, reg: &str) -> &'static str {
        match reg {
            "rax" | "eax" => "al",
            "rcx" | "ecx" => "cl",
            "rdx" | "edx" => "dl",
            "rbx" | "ebx" => "bl",
            "rsi" | "esi" => "sil",
            "rdi" | "edi" => "dil",
            "rsp" | "esp" => "spl",
            "rbp" | "ebp" => "bpl",
            "r8" | "r8d" => "r8b",
            "r9" | "r9d" => "r9b",
            "r10" | "r10d" => "r10b",
            "r11" | "r11d" => "r11b",
            "r12" | "r12d" => "r12b",
            "r13" | "r13d" => "r13b",
            "r14" | "r14d" => "r14b",
            "r15" | "r15d" => "r15b",
            _ => "al",
        }
    }

    /// Get the word-sized version of any x86 register.
    pub(super) fn reg_word(&self, reg: &str) -> &'static str {
        match reg {
            "rax" | "eax" => "ax",
            "rcx" | "ecx" => "cx",
            "rdx" | "edx" => "dx",
            "rbx" | "ebx" => "bx",
            "rsi" | "esi" => "si",
            "rdi" | "edi" => "di",
            "rsp" | "esp" => "sp",
            "rbp" | "ebp" => "bp",
            "r8" | "r8d" => "r8w",
            "r9" | "r9d" => "r9w",
            "r10" | "r10d" => "r10w",
            "r11" | "r11d" => "r11w",
            "r12" | "r12d" => "r12w",
            "r13" | "r13d" => "r13w",
            "r14" | "r14d" => "r14w",
            "r15" | "r15d" => "r15w",
            _ => "ax",
        }
    }

    /// Get the dword-sized version of any x86 register.
    pub(super) fn reg_dword(&self, reg: &str) -> &'static str {
        match reg {
            "rax" | "eax" => "eax",
            "rcx" | "ecx" => "ecx",
            "rdx" | "edx" => "edx",
            "rbx" | "ebx" => "ebx",
            "rsi" | "esi" => "esi",
            "rdi" | "edi" => "edi",
            "rsp" | "esp" => "esp",
            "rbp" | "ebp" => "ebp",
            "r8" | "r8d" => "r8d",
            "r9" | "r9d" => "r9d",
            "r10" | "r10d" => "r10d",
            "r11" | "r11d" => "r11d",
            "r12" | "r12d" => "r12d",
            "r13" | "r13d" => "r13d",
            "r14" | "r14d" => "r14d",
            "r15" | "r15d" => "r15d",
            _ => "eax",
        }
    }

    /// Get the qword-sized version of any x86 register.
    pub(super) fn reg_qword(&self, reg: &str) -> &'static str {
        match reg {
            "rax" | "eax" => "rax",
            "rcx" | "ecx" => "rcx",
            "rdx" | "edx" => "rdx",
            "rbx" | "ebx" => "rbx",
            "rsi" | "esi" => "rsi",
            "rdi" | "edi" => "rdi",
            "rsp" | "esp" => "rsp",
            "rbp" | "ebp" => "rbp",
            "r8" | "r8d" => "r8",
            "r9" | "r9d" => "r9",
            "r10" | "r10d" => "r10",
            "r11" | "r11d" => "r11",
            "r12" | "r12d" => "r12",
            "r13" | "r13d" => "r13",
            "r14" | "r14d" => "r14",
            "r15" | "r15d" => "r15",
            _ => "rax",
        }
    }

    // ========================================================================
    // Register access helpers
    // ========================================================================

    /// Get the x86 register for a RISC-V register (hot regs only).
    /// Returns None for cold registers.
    pub(super) fn rv_reg(&self, reg: u8) -> Option<&'static str> {
        if reg == 0 {
            return None;
        }
        self.reg_map.get(reg)
    }

    /// Load a RISC-V register value into a temporary x86 register.
    /// Returns the register name to use (without % prefix).
    pub(super) fn load_rv_to_temp(&mut self, rv_reg: u8, temp: &str) -> String {
        if rv_reg == 0 {
            self.emitf(format!("xor{} %{temp}, %{temp}", self.suffix()));
            return temp.to_string();
        }
        if let Some(x86_reg) = self.reg_map.get(rv_reg) {
            // Already in a register
            x86_reg.to_string()
        } else {
            // Load from memory
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "mov{} {}(%{}), %{temp}",
                self.suffix(),
                offset,
                reserved::STATE_PTR
            ));
            temp.to_string()
        }
    }

    /// Load a RISC-V register value as a 64-bit address.
    /// For RV32, zero-extends to 64-bit. For RV64, loads directly.
    /// Always returns a 64-bit register name.
    pub(super) fn load_rv_as_addr(&mut self, rv_reg: u8, temp64: &str) -> String {
        if rv_reg == 0 {
            self.emitf(format!("xorq %{temp64}, %{temp64}"));
            return temp64.to_string();
        }
        if let Some(x86_reg) = self.reg_map.get(rv_reg) {
            if X::VALUE == 32 {
                // RV32: hot regs are 32-bit, zero-extend to 64-bit
                let reg32 = self.temp_dword(temp64);
                self.emitf(format!("movl %{x86_reg}, %{reg32}"));
                self.temp_qword(temp64).to_string()
            } else {
                x86_reg.to_string()
            }
        } else {
            let offset = self.layout.reg_offset(rv_reg);
            if X::VALUE == 32 {
                let reg32 = self.temp_dword(temp64);
                self.emitf(format!(
                    "movl {}(%{}), %{reg32}",
                    offset,
                    reserved::STATE_PTR
                ));
            } else {
                self.emitf(format!(
                    "movq {}(%{}), %{temp64}",
                    offset,
                    reserved::STATE_PTR
                ));
            }
            temp64.to_string()
        }
    }

    /// Store a value to a RISC-V register.
    /// `value` is the register name without % prefix.
    pub(super) fn store_to_rv(&mut self, rv_reg: u8, value: &str) {
        if rv_reg == 0 {
            return; // x0 ignores writes
        }
        if let Some(x86_reg) = self.reg_map.get(rv_reg) {
            if x86_reg != value {
                self.emitf(format!("mov{} %{value}, %{x86_reg}", self.suffix()));
            }
        } else {
            let offset = self.layout.reg_offset(rv_reg);
            self.emitf(format!(
                "mov{} %{value}, {}(%{})",
                self.suffix(),
                offset,
                reserved::STATE_PTR
            ));
        }
    }

    // ========================================================================
    // Address translation
    // ========================================================================

    /// Apply address translation to an address in temp register.
    /// `temp` is the register name without % prefix.
    ///
    /// Uses AddressMode semantics:
    /// - Unchecked: no-op (guard pages catch OOB)
    /// - Wrap: mask address to memory size
    /// - Bounds: check bounds, trap if OOB, then mask
    pub(super) fn apply_address_mode(&mut self, temp: &str) {
        let mode = self.config.address_mode;

        // Bounds check (Bounds mode only)
        if mode.needs_bounds_check() {
            let ok_label = self.next_label("bounds_ok");
            let shift_amount = X::VALUE as u32 - self.config.memory_bits as u32;
            if X::VALUE == 32 {
                let temp32 = self.temp_dword(temp);
                self.emitf(format!("movl %{temp32}, %edx"));
                self.emitf(format!("shll ${}, %edx", shift_amount));
                self.emitf(format!("sarl ${}, %edx", shift_amount));
                self.emitf(format!("cmpl %{temp32}, %edx"));
            } else {
                self.emitf(format!("movq %{temp}, %rdx"));
                self.emitf(format!("shlq ${}, %rdx", shift_amount));
                self.emitf(format!("sarq ${}, %rdx", shift_amount));
                self.emitf(format!("cmpq %{temp}, %rdx"));
            }
            self.emitf(format!("je {ok_label}"));
            self.emit("jmp asm_trap");
            self.emit_label(&ok_label);
        }

        // Address masking (Wrap and Bounds modes)
        if mode.needs_mask() {
            let mask = self.memory_mask;
            if X::VALUE == 64 && mask == 0xffffffff {
                // Zero upper 32 bits by moving 32-bit to itself
                let temp32 = self.temp_dword(temp);
                self.emitf(format!("movl %{temp32}, %{temp32}"));
            } else if mask <= 0x7fffffff {
                self.emitf(format!("and{} $0x{:x}, %{temp}", self.suffix(), mask));
            } else {
                self.emitf(format!("movl $0x{:x}, %edx", mask as u32));
                self.emitf(format!("andq %rdx, %{temp}"));
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
            "addq ${}, {}(%{})",
            count,
            instret_offset,
            reserved::STATE_PTR
        ));

        // For suspend mode, check if we hit the limit
        if self.config.instret_mode.suspends() {
            let continue_label = self.next_label("instret_ok");
            let target_offset = self.layout.offset_target_instret;
            self.emitf(format!(
                "movq {}(%{}), %rdx",
                target_offset,
                reserved::STATE_PTR
            ));
            self.emitf(format!(
                "cmpq %rdx, {}(%{})",
                instret_offset,
                reserved::STATE_PTR
            ));
            self.emitf(format!("jb {continue_label}"));
            // Suspend: save current PC
            let pc_offset = self.layout.offset_pc;
            if X::VALUE == 32 {
                self.emitf(format!(
                    "movl $0x{:x}, {}(%{})",
                    pc as u32,
                    pc_offset,
                    reserved::STATE_PTR
                ));
            } else {
                // For 64-bit immediates > 32-bit, need movabs
                if pc > 0x7fffffff {
                    self.emitf(format!("movabsq $0x{:x}, %rdx", pc));
                    self.emitf(format!(
                        "movq %rdx, {}(%{})",
                        pc_offset,
                        reserved::STATE_PTR
                    ));
                } else {
                    self.emitf(format!(
                        "movq $0x{:x}, {}(%{})",
                        pc,
                        pc_offset,
                        reserved::STATE_PTR
                    ));
                }
            }
            self.emit("jmp asm_exit");
            self.emit_label(&continue_label);
        }
    }
}
