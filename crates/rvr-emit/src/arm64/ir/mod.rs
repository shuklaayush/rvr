//! IR translation for ARM64 assembly.
//!
//! Translates IR expressions, statements, and terminators to ARM64 assembly.

use rvr_ir::{Stmt, WriteTarget, Xlen};

mod expr;
mod stmt;
mod terminator;

/// Check if a statement (recursively) writes to Exited.
fn stmt_writes_to_exited<X: Xlen>(stmt: &Stmt<X>) -> bool {
    match stmt {
        Stmt::Write { target, .. } => matches!(target, WriteTarget::Exited),
        Stmt::If {
            then_stmts,
            else_stmts,
            ..
        } => {
            then_stmts.iter().any(stmt_writes_to_exited)
                || else_stmts.iter().any(stmt_writes_to_exited)
        }
        Stmt::ExternCall { .. } => false,
    }
}
