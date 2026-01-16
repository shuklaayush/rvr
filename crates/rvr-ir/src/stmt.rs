//! Statement IR.

use crate::xlen::Xlen;

use crate::expr::{Expr, Space};

/// Statement kinds.
#[derive(Clone, Debug)]
pub enum Stmt<X: Xlen> {
    /// Write to register/memory/CSR.
    Write {
        space: Space,
        addr: Expr<X>,
        value: Expr<X>,
        width: u8,
    },
    /// Conditional execution.
    If {
        cond: Expr<X>,
        then_stmts: Vec<Stmt<X>>,
        else_stmts: Vec<Stmt<X>>,
    },
    /// External function call (for side effects).
    ExternCall { fn_name: String, args: Vec<Expr<X>> },
}

impl<X: Xlen> Stmt<X> {
    /// Create a register write statement.
    pub fn write_reg(reg: u8, value: Expr<X>) -> Self {
        Self::Write {
            space: Space::Reg,
            addr: Expr::imm(X::from_u64(reg as u64)),
            value,
            width: X::REG_BYTES as u8,
        }
    }

    /// Create a memory write statement.
    pub fn write_mem(addr: Expr<X>, value: Expr<X>, width: u8) -> Self {
        Self::Write {
            space: Space::Mem,
            addr,
            value,
            width,
        }
    }

    /// Create a CSR write statement.
    pub fn write_csr(csr: u16, value: Expr<X>) -> Self {
        Self::Write {
            space: Space::Csr,
            addr: Expr::imm(X::from_u64(csr as u64)),
            value,
            width: X::REG_BYTES as u8,
        }
    }

    /// Create an external call statement.
    pub fn extern_call(fn_name: &str, args: Vec<Expr<X>>) -> Self {
        Self::ExternCall {
            fn_name: fn_name.to_string(),
            args,
        }
    }

    /// Create an if statement.
    pub fn if_then(cond: Expr<X>, then_stmts: Vec<Self>) -> Self {
        Self::If {
            cond,
            then_stmts,
            else_stmts: Vec::new(),
        }
    }

    /// Create an if-else statement.
    pub fn if_then_else(cond: Expr<X>, then_stmts: Vec<Self>, else_stmts: Vec<Self>) -> Self {
        Self::If {
            cond,
            then_stmts,
            else_stmts,
        }
    }
}
