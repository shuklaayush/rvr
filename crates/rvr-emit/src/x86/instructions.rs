//! RISC-V instruction emission for x86-64 assembly (AT&T syntax).
//!
//! Emits x86-64 assembly for individual RISC-V instructions.

use rvr_ir::Xlen;

use super::X86Emitter;
use super::registers::reserved;

impl<X: Xlen> X86Emitter<X> {
    /// Emit an ADD instruction: rd = rs1 + rs2
    pub fn emit_add(&mut self, rd: u8, rs1: u8, rs2: u8) {
        if rd == 0 {
            return;
        }

        let temp1 = self.temp1();
        let suffix = self.suffix();
        let src1 = self.load_rv_to_temp(rs1, temp1);

        if rs2 == 0 {
            self.store_to_rv(rd, &src1);
            return;
        }

        let src2 = if let Some(r) = self.rv_reg(rs2) {
            r.to_string()
        } else {
            let temp2 = self.temp2();
            self.load_rv_to_temp(rs2, temp2)
        };

        if rd == rs1 && self.reg_map.is_hot(rd) {
            self.emitf(format!("add{suffix} %{src2}, %{src1}"));
        } else {
            if src1 != temp1 {
                self.emitf(format!("mov{suffix} %{src1}, %{temp1}"));
            }
            self.emitf(format!("add{suffix} %{src2}, %{temp1}"));
            self.store_to_rv(rd, temp1);
        }
    }

    /// Emit a SUB instruction: rd = rs1 - rs2
    pub fn emit_sub(&mut self, rd: u8, rs1: u8, rs2: u8) {
        if rd == 0 {
            return;
        }

        let temp1 = self.temp1();
        let suffix = self.suffix();
        let src1 = self.load_rv_to_temp(rs1, temp1);

        if rs2 == 0 {
            self.store_to_rv(rd, &src1);
            return;
        }

        let src2 = if let Some(r) = self.rv_reg(rs2) {
            r.to_string()
        } else {
            let temp2 = self.temp2();
            self.load_rv_to_temp(rs2, temp2)
        };

        // In-place optimization: if rd == rs1 and hot, modify in place
        if rd == rs1 && self.reg_map.is_hot(rd) {
            self.emitf(format!("sub{suffix} %{src2}, %{src1}"));
        } else {
            if src1 != temp1 {
                self.emitf(format!("mov{suffix} %{src1}, %{temp1}"));
            }
            self.emitf(format!("sub{suffix} %{src2}, %{temp1}"));
            self.store_to_rv(rd, temp1);
        }
    }

    /// Emit an ADDI instruction: rd = rs1 + imm
    pub fn emit_addi(&mut self, rd: u8, rs1: u8, imm: i32) {
        if rd == 0 {
            return;
        }

        let temp1 = self.temp1();
        let suffix = self.suffix();

        if imm == 0 {
            if rs1 == 0 {
                self.emitf(format!("xor{suffix} %{temp1}, %{temp1}"));
                self.store_to_rv(rd, temp1);
            } else if let Some(src) = self.rv_reg(rs1) {
                self.store_to_rv(rd, src);
            } else {
                let src = self.load_rv_to_temp(rs1, temp1);
                self.store_to_rv(rd, &src);
            }
            return;
        }

        // In-place optimization: if rd == rs1 and hot, modify in place
        if rd == rs1 && self.reg_map.is_hot(rd) {
            let src = self.rv_reg(rs1).unwrap();
            self.emitf(format!("add{suffix} ${imm}, %{src}"));
            return;
        }

        // LEA optimization: use lea for src + imm -> dest (single instruction)
        // Works when source is in a register and we're writing to a different dest
        if self.reg_map.is_hot(rs1) {
            // Source is hot register - use LEA to add immediate
            let src64 = self.reg_map.get_64(rs1).unwrap();
            if X::VALUE == 32 {
                self.emitf(format!("leal {imm}(%{src64}), %{temp1}"));
            } else {
                self.emitf(format!("leaq {imm}(%{src64}), %{temp1}"));
            }
            self.store_to_rv(rd, temp1);
        } else if rs1 == 0 {
            // Source is x0 (zero) - just load immediate
            if X::VALUE == 32 {
                self.emitf(format!("movl ${imm}, %{temp1}"));
            } else {
                self.emitf(format!("movq ${imm}, %{temp1}"));
            }
            self.store_to_rv(rd, temp1);
        } else {
            // Source is cold register - load then add
            let src = self.load_rv_to_temp(rs1, temp1);
            if src != temp1 {
                self.emitf(format!("mov{suffix} %{src}, %{temp1}"));
            }
            self.emitf(format!("add{suffix} ${imm}, %{temp1}"));
            self.store_to_rv(rd, temp1);
        }
    }

    /// Emit a LUI instruction: rd = imm << 12
    pub fn emit_lui(&mut self, rd: u8, imm: i32) {
        if rd == 0 {
            return;
        }
        let temp1 = self.temp1();
        let value = (imm as i64) << 12;
        if X::VALUE == 32 {
            self.emitf(format!("movl ${}, %{temp1}", value as i32));
        } else {
            // For 64-bit values that don't fit in 32-bit signed, use movabs
            if value > i32::MAX as i64 || value < i32::MIN as i64 {
                self.emitf(format!("movabsq ${}, %{temp1}", value));
            } else {
                self.emitf(format!("movq ${}, %{temp1}", value));
            }
        }
        self.store_to_rv(rd, temp1);
    }

    /// Emit an AUIPC instruction: rd = pc + (imm << 12)
    pub fn emit_auipc(&mut self, rd: u8, imm: i32, pc: u64) {
        if rd == 0 {
            return;
        }
        let temp1 = self.temp1();
        let value = pc.wrapping_add(((imm as i64) << 12) as u64);
        if X::VALUE == 32 {
            self.emitf(format!("movl $0x{:x}, %{temp1}", value as u32));
        } else {
            if value > 0x7fffffff {
                self.emitf(format!("movabsq $0x{:x}, %{temp1}", value));
            } else {
                self.emitf(format!("movq $0x{:x}, %{temp1}", value));
            }
        }
        self.store_to_rv(rd, temp1);
    }

    /// Emit a branch instruction.
    pub fn emit_branch(&mut self, op: &str, rs1: u8, rs2: u8, target: u64) {
        let temp1 = self.temp1();
        let temp2 = self.temp2();
        let suffix = self.suffix();
        let src1 = self.load_rv_to_temp(rs1, temp1);
        let src2 = self.load_rv_to_temp(rs2, temp2);

        self.emitf(format!("cmp{suffix} %{src2}, %{src1}"));

        let jcc = match op {
            "beq" => "je",
            "bne" => "jne",
            "blt" => "jl",
            "bge" => "jge",
            "bltu" => "jb",
            "bgeu" => "jae",
            _ => "jmp",
        };

        self.emitf(format!("{jcc} asm_pc_{:x}", target));
    }

    /// Emit a JAL instruction: rd = pc + 4; jump to target
    pub fn emit_jal(&mut self, rd: u8, target: u64, next_pc: u64) {
        if rd != 0 {
            let temp1 = self.temp1();
            if X::VALUE == 32 {
                self.emitf(format!("movl $0x{:x}, %{temp1}", next_pc as u32));
            } else {
                if next_pc > 0x7fffffff {
                    self.emitf(format!("movabsq $0x{:x}, %{temp1}", next_pc));
                } else {
                    self.emitf(format!("movq $0x{:x}, %{temp1}", next_pc));
                }
            }
            self.store_to_rv(rd, temp1);
        }
        self.emitf(format!("jmp asm_pc_{:x}", target));
    }

    /// Emit a JALR instruction: rd = pc + 4; jump to (rs1 + imm) & ~1
    pub fn emit_jalr(&mut self, rd: u8, rs1: u8, imm: i32, next_pc: u64) {
        let temp1 = self.temp1();

        // Save return address first
        if rd != 0 {
            if X::VALUE == 32 {
                self.emitf(format!("movl $0x{:x}, %{temp1}", next_pc as u32));
            } else {
                if next_pc > 0x7fffffff {
                    self.emitf(format!("movabsq $0x{:x}, %{temp1}", next_pc));
                } else {
                    self.emitf(format!("movq $0x{:x}, %{temp1}", next_pc));
                }
            }
            self.store_to_rv(rd, temp1);
        }

        // Compute target address (use rax for address calculation)
        let base = self.load_rv_as_addr(rs1, "rax");
        if imm != 0 {
            self.emitf(format!("leaq {imm}(%{base}), %rax"));
        } else if base != "rax" {
            self.emitf(format!("movq %{base}, %rax"));
        }
        self.emit("andq $-2, %rax"); // Clear low bit

        // Jump via dispatch table
        self.emit_dispatch_jump();
    }

    /// Emit a load instruction.
    pub fn emit_load(&mut self, rd: u8, rs1: u8, offset: i32, width: u8, signed: bool) {
        if rd == 0 {
            return;
        }

        // Compute effective address into rax (always 64-bit for addressing)
        let base = self.load_rv_as_addr(rs1, "rax");

        // Compute full effective address
        if offset != 0 {
            self.emitf(format!("leaq {offset}(%{base}), %rax"));
        } else if base != "rax" {
            self.emitf(format!("movq %{base}, %rax"));
        }

        // Apply address translation
        self.apply_address_mode("rax");

        // Perform load with correct instruction
        let temp1 = self.temp1();
        let mem_op = format!("(%{}, %rax)", reserved::MEMORY_PTR);

        match (width, signed, X::VALUE) {
            (1, false, _) => {
                self.emitf(format!("movzbl {mem_op}, %{}", self.reg_dword(temp1)));
            }
            (1, true, 32) => {
                self.emitf(format!("movsbl {mem_op}, %{temp1}"));
            }
            (1, true, 64) => {
                self.emitf(format!("movsbq {mem_op}, %{temp1}"));
            }
            (2, false, _) => {
                self.emitf(format!("movzwl {mem_op}, %{}", self.reg_dword(temp1)));
            }
            (2, true, 32) => {
                self.emitf(format!("movswl {mem_op}, %{temp1}"));
            }
            (2, true, 64) => {
                self.emitf(format!("movswq {mem_op}, %{temp1}"));
            }
            (4, _, 32) => {
                self.emitf(format!("movl {mem_op}, %{temp1}"));
            }
            (4, false, 64) => {
                // mov to 32-bit reg zero-extends
                self.emitf(format!("movl {mem_op}, %{}", self.reg_dword(temp1)));
            }
            (4, true, 64) => {
                self.emitf(format!("movslq {mem_op}, %{temp1}"));
            }
            (8, _, _) => {
                self.emitf(format!("movq {mem_op}, %{temp1}"));
            }
            _ => panic!("invalid load width"),
        }

        self.store_to_rv(rd, temp1);
    }

    /// Emit a store instruction.
    pub fn emit_store(&mut self, rs1: u8, rs2: u8, offset: i32, width: u8) {
        // Get value to store into temp2 FIRST
        let temp2 = self.temp2();
        let value = self.load_rv_to_temp(rs2, temp2);

        // Move to temp2 if not already there
        let val_reg_base = if value != temp2 && value != "rcx" && value != "ecx" {
            self.emitf(format!("mov{} %{value}, %{temp2}", self.suffix()));
            temp2
        } else {
            &value
        };

        let val_reg = match width {
            1 => self.reg_byte(val_reg_base),
            2 => self.reg_word(val_reg_base),
            4 => self.reg_dword(val_reg_base),
            8 => self.reg_qword(val_reg_base),
            _ => panic!("invalid store width"),
        };

        // Compute effective address into rax
        let base = self.load_rv_as_addr(rs1, "rax");

        if offset != 0 {
            self.emitf(format!("leaq {offset}(%{base}), %rax"));
        } else if base != "rax" {
            self.emitf(format!("movq %{base}, %rax"));
        }

        // Apply address translation
        self.apply_address_mode("rax");

        let mem_op = format!("(%{}, %rax)", reserved::MEMORY_PTR);
        let suffix = match width {
            1 => "b",
            2 => "w",
            4 => "l",
            8 => "q",
            _ => panic!("invalid store width"),
        };

        self.emitf(format!("mov{suffix} %{val_reg}, {mem_op}"));
    }

    /// Emit an ECALL instruction.
    pub fn emit_ecall(&mut self, pc: u64) {
        let pc_offset = self.layout.offset_pc;
        let has_exited = self.layout.offset_has_exited;
        let exit_code = self.layout.offset_exit_code;

        // Save PC to state
        if X::VALUE == 32 {
            self.emitf(format!(
                "movl $0x{:x}, {}(%{})",
                pc as u32,
                pc_offset,
                reserved::STATE_PTR
            ));
        } else {
            if pc > 0x7fffffff {
                self.emitf(format!("movabsq $0x{:x}, %rax", pc));
                self.emitf(format!(
                    "movq %rax, {}(%{})",
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

        // Exit to let runtime handle syscall (2 = syscall)
        self.emitf(format!("movb $2, {}(%{})", has_exited, reserved::STATE_PTR));
        self.emitf(format!("movb $2, {}(%{})", exit_code, reserved::STATE_PTR));
        self.emit("jmp asm_exit");
    }

    /// Emit an EBREAK instruction.
    pub fn emit_ebreak(&mut self, pc: u64) {
        let pc_offset = self.layout.offset_pc;
        let has_exited = self.layout.offset_has_exited;
        let exit_code = self.layout.offset_exit_code;

        // Save PC
        if X::VALUE == 32 {
            self.emitf(format!(
                "movl $0x{:x}, {}(%{})",
                pc as u32,
                pc_offset,
                reserved::STATE_PTR
            ));
        } else {
            if pc > 0x7fffffff {
                self.emitf(format!("movabsq $0x{:x}, %rax", pc));
                self.emitf(format!(
                    "movq %rax, {}(%{})",
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

        // Exit with breakpoint indication (3 = breakpoint)
        self.emitf(format!("movb $3, {}(%{})", has_exited, reserved::STATE_PTR));
        self.emitf(format!("movb $3, {}(%{})", exit_code, reserved::STATE_PTR));
        self.emit("jmp asm_exit");
    }
}
