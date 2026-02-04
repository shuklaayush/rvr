//! Statement IR.

use crate::xlen::Xlen;

use crate::expr::Expr;

/// Write target for statements.
#[derive(Clone, Debug)]
pub enum WriteTarget<X: Xlen> {
    /// Register write (index 0-31).
    Reg(u8),
    /// CSR write.
    Csr(u16),
    /// Memory write with base register, constant offset, and width.
    Mem {
        base: Expr<X>,
        offset: i16,
        width: u8,
    },
    /// PC update.
    Pc,
    /// Temporary variable.
    Temp(u8),
    /// Reservation address (LR/SC).
    ResAddr,
    /// Reservation valid flag.
    ResValid,
    /// Exit flag.
    Exited,
    /// Exit code.
    ExitCode,
}

/// Statement kinds.
#[derive(Clone, Debug)]
pub enum Stmt<X: Xlen> {
    /// Write to a target.
    Write {
        target: WriteTarget<X>,
        value: Expr<X>,
    },
    /// Conditional execution.
    If {
        cond: Expr<X>,
        then_stmts: Vec<Self>,
        else_stmts: Vec<Self>,
    },
    /// External function call (for side effects).
    ExternCall { fn_name: String, args: Vec<Expr<X>> },
}

impl<X: Xlen> Stmt<X> {
    /// Create a register write statement.
    pub const fn write_reg(reg: u8, value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::Reg(reg),
            value,
        }
    }

    /// Create a memory write statement with base register and constant offset.
    pub const fn write_mem(base: Expr<X>, offset: i16, value: Expr<X>, width: u8) -> Self {
        Self::Write {
            target: WriteTarget::Mem {
                base,
                offset,
                width,
            },
            value,
        }
    }

    /// Create a memory write statement with computed address (offset = 0).
    pub const fn write_mem_addr(addr: Expr<X>, value: Expr<X>, width: u8) -> Self {
        Self::Write {
            target: WriteTarget::Mem {
                base: addr,
                offset: 0,
                width,
            },
            value,
        }
    }

    /// Create a CSR write statement.
    pub const fn write_csr(csr: u16, value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::Csr(csr),
            value,
        }
    }

    /// Create a PC write statement.
    pub const fn write_pc(value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::Pc,
            value,
        }
    }

    /// Create a temporary variable write.
    pub const fn write_temp(idx: u8, value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::Temp(idx),
            value,
        }
    }

    /// Create a reservation address write.
    pub const fn write_res_addr(value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::ResAddr,
            value,
        }
    }

    /// Create a reservation valid flag write.
    pub const fn write_res_valid(value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::ResValid,
            value,
        }
    }

    /// Create an exit flag write.
    pub const fn write_exited(value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::Exited,
            value,
        }
    }

    /// Create an exit code write.
    pub const fn write_exit_code(value: Expr<X>) -> Self {
        Self::Write {
            target: WriteTarget::ExitCode,
            value,
        }
    }

    /// Create an external call statement.
    #[must_use]
    pub fn extern_call(fn_name: &str, args: Vec<Expr<X>>) -> Self {
        Self::ExternCall {
            fn_name: fn_name.to_string(),
            args,
        }
    }

    /// Create an if statement.
    pub const fn if_then(cond: Expr<X>, then_stmts: Vec<Self>) -> Self {
        Self::If {
            cond,
            then_stmts,
            else_stmts: Vec::new(),
        }
    }

    /// Create an if-else statement.
    pub const fn if_then_else(cond: Expr<X>, then_stmts: Vec<Self>, else_stmts: Vec<Self>) -> Self {
        Self::If {
            cond,
            then_stmts,
            else_stmts,
        }
    }
}
