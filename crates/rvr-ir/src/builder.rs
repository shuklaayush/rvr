//! IR builder fluent API.

use crate::expr::Expr;
use crate::instr::InstrIR;
use crate::stmt::Stmt;
use crate::terminator::Terminator;
use crate::xlen::Xlen;

/// Builder for instruction IR.
pub struct IRBuilder<X: Xlen> {
    pc: X::Reg,
    size: u8,
    op: u16,
    raw: u32,
    statements: Vec<Stmt<X>>,
}

impl<X: Xlen> IRBuilder<X> {
    /// Create a new IR builder.
    pub const fn new(pc: X::Reg, size: u8) -> Self {
        Self {
            pc,
            size,
            op: 0,
            raw: 0,
            statements: Vec::new(),
        }
    }

    /// Set packed `OpId` (ext << 8 | idx) for tracing.
    #[must_use]
    pub const fn with_op(mut self, op: u16) -> Self {
        self.op = op;
        self
    }

    /// Set raw instruction bytes for tracing.
    #[must_use]
    pub const fn with_raw(mut self, raw: u32) -> Self {
        self.raw = raw;
        self
    }

    /// Write to a register.
    #[must_use]
    pub fn write_reg(mut self, rd: u8, value: Expr<X>) -> Self {
        if rd != 0 {
            self.statements.push(Stmt::write_reg(rd, value));
        }
        self
    }

    /// Write to memory with base register and constant offset.
    #[must_use]
    pub fn write_mem(mut self, base: Expr<X>, offset: i16, value: Expr<X>, width: u8) -> Self {
        self.statements
            .push(Stmt::write_mem(base, offset, value, width));
        self
    }

    /// Write to memory with computed address (offset = 0).
    #[must_use]
    pub fn write_mem_addr(mut self, addr: Expr<X>, value: Expr<X>, width: u8) -> Self {
        self.statements
            .push(Stmt::write_mem_addr(addr, value, width));
        self
    }

    /// Write to a CSR.
    #[must_use]
    pub fn write_csr(mut self, csr: u16, value: Expr<X>) -> Self {
        self.statements.push(Stmt::write_csr(csr, value));
        self
    }

    /// Add an external call (for side effects).
    #[must_use]
    pub fn extern_call(mut self, fn_name: &str, args: Vec<Expr<X>>) -> Self {
        self.statements.push(Stmt::extern_call(fn_name, args));
        self
    }

    /// Call external function and store result in rd.
    #[must_use]
    pub fn extern_call_to_reg(mut self, rd: u8, fn_name: &str, args: Vec<Expr<X>>) -> Self {
        if rd != 0 {
            let width = u8::try_from(X::REG_BYTES).unwrap_or(0);
            let call = Expr::extern_call(fn_name, args, width);
            self.statements.push(Stmt::write_reg(rd, call));
        }
        self
    }

    /// Add a conditional statement.
    #[must_use]
    pub fn if_then(mut self, cond: Expr<X>, then_stmts: Vec<Stmt<X>>) -> Self {
        self.statements.push(Stmt::if_then(cond, then_stmts));
        self
    }

    /// Add a raw statement.
    #[must_use]
    pub fn stmt(mut self, stmt: Stmt<X>) -> Self {
        self.statements.push(stmt);
        self
    }

    /// Build with fall-through terminator.
    pub fn build_fall(self) -> InstrIR<X> {
        let next_pc = X::from_u64(X::to_u64(self.pc) + u64::from(self.size));
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            Terminator::fall(next_pc),
        )
    }

    /// Build with fall-through terminator (no explicit target).
    pub fn build_fall_no_target(self) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            Terminator::Fall { target: None },
        )
    }

    /// Build with static jump terminator.
    pub fn build_jump(self, target: X::Reg) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            Terminator::jump(target),
        )
    }

    /// Build with dynamic jump terminator.
    pub fn build_jump_dyn(self, addr: Expr<X>) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            Terminator::jump_dyn(addr),
        )
    }

    /// Build with conditional branch terminator.
    pub fn build_branch(self, cond: Expr<X>, target: X::Reg) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            Terminator::branch(cond, target),
        )
    }

    /// Build with exit terminator.
    pub fn build_exit(self, code: Expr<X>) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            Terminator::exit(code),
        )
    }

    /// Build with trap terminator.
    pub fn build_trap(self, message: &str) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            Terminator::trap(message),
        )
    }

    /// Build with custom terminator.
    pub fn build(self, terminator: Terminator<X>) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.op,
            self.raw,
            self.statements,
            terminator,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xlen::Rv64;

    #[test]
    fn test_builder() {
        let ir = IRBuilder::<Rv64>::new(0x8000_0000_u64, 4)
            .write_reg(1, Expr::imm(42))
            .build_fall();

        assert_eq!(ir.pc, 0x8000_0000);
        assert_eq!(ir.size, 4);
        assert_eq!(ir.statements.len(), 1);
        assert!(ir.terminator.is_fall());
    }
}
