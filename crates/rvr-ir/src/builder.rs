//! IR builder fluent API.

use rvr_isa::{OpId, Xlen};

use crate::expr::Expr;
use crate::instr::InstrIR;
use crate::stmt::Stmt;
use crate::terminator::Terminator;

/// Builder for instruction IR.
pub struct IRBuilder<X: Xlen> {
    pc: X::Reg,
    size: u8,
    opid: OpId,
    statements: Vec<Stmt<X>>,
}

impl<X: Xlen> IRBuilder<X> {
    /// Create a new IR builder.
    pub fn new(pc: X::Reg, size: u8, opid: OpId) -> Self {
        Self {
            pc,
            size,
            opid,
            statements: Vec::new(),
        }
    }

    /// Write to a register.
    pub fn write_reg(mut self, rd: u8, value: Expr<X>) -> Self {
        if rd != 0 {
            self.statements.push(Stmt::write_reg(rd, value));
        }
        self
    }

    /// Write to memory.
    pub fn write_mem(mut self, addr: Expr<X>, value: Expr<X>, width: u8) -> Self {
        self.statements.push(Stmt::write_mem(addr, value, width));
        self
    }

    /// Write to a CSR.
    pub fn write_csr(mut self, csr: u16, value: Expr<X>) -> Self {
        self.statements.push(Stmt::write_csr(csr, value));
        self
    }

    /// Add an external call (for side effects).
    pub fn extern_call(mut self, fn_name: &str, args: Vec<Expr<X>>) -> Self {
        self.statements.push(Stmt::extern_call(fn_name, args));
        self
    }

    /// Call external function and store result in rd.
    pub fn extern_call_to_reg(mut self, rd: u8, fn_name: &str, args: Vec<Expr<X>>) -> Self {
        if rd != 0 {
            let call = Expr::extern_call(fn_name, args, X::REG_BYTES as u8);
            self.statements.push(Stmt::write_reg(rd, call));
        }
        self
    }

    /// Add a conditional statement.
    pub fn if_then(mut self, cond: Expr<X>, then_stmts: Vec<Stmt<X>>) -> Self {
        self.statements.push(Stmt::if_then(cond, then_stmts));
        self
    }

    /// Add a raw statement.
    pub fn stmt(mut self, stmt: Stmt<X>) -> Self {
        self.statements.push(stmt);
        self
    }

    /// Build with fall-through terminator.
    pub fn build_fall(self) -> InstrIR<X> {
        InstrIR::new(self.pc, self.size, self.opid, self.statements, Terminator::Fall)
    }

    /// Build with static jump terminator.
    pub fn build_jump(self, target: X::Reg) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.opid,
            self.statements,
            Terminator::jump(target),
        )
    }

    /// Build with dynamic jump terminator.
    pub fn build_jump_dyn(self, addr: Expr<X>) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.opid,
            self.statements,
            Terminator::jump_dyn(addr),
        )
    }

    /// Build with conditional branch terminator.
    pub fn build_branch(self, cond: Expr<X>, target: X::Reg) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.opid,
            self.statements,
            Terminator::branch(cond, target),
        )
    }

    /// Build with exit terminator.
    pub fn build_exit(self, code: Expr<X>) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.opid,
            self.statements,
            Terminator::exit(code),
        )
    }

    /// Build with trap terminator.
    pub fn build_trap(self, message: &str) -> InstrIR<X> {
        InstrIR::new(
            self.pc,
            self.size,
            self.opid,
            self.statements,
            Terminator::trap(message),
        )
    }

    /// Build with custom terminator.
    pub fn build(self, terminator: Terminator<X>) -> InstrIR<X> {
        InstrIR::new(self.pc, self.size, self.opid, self.statements, terminator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_isa::{Rv64, EXT_I};

    #[test]
    fn test_builder() {
        let opid = OpId::new(EXT_I, 0);
        let ir = IRBuilder::<Rv64>::new(0x80000000u64, 4, opid)
            .write_reg(1, Expr::imm(42))
            .build_fall();

        assert_eq!(ir.pc, 0x80000000);
        assert_eq!(ir.size, 4);
        assert_eq!(ir.statements.len(), 1);
        assert!(ir.terminator.is_fall());
    }
}
