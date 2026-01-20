//! Table-driven syscall lowering.

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use crate::{DecodedInstr, REG_A0, REG_A7, REG_T0};

/// Syscall ABI for the syscall number register.
#[derive(Clone, Copy, Debug)]
pub enum SyscallAbi {
    /// Standard RISC-V ABI: syscall number in a7.
    Standard,
    /// RVE ABI (16 regs): syscall number in t0 (x5).
    Embedded,
}

impl SyscallAbi {
    #[inline]
    pub fn syscall_reg(self) -> u8 {
        match self {
            SyscallAbi::Standard => REG_A7,
            SyscallAbi::Embedded => REG_T0,
        }
    }
}

/// Trait for handling ECALL instructions.
pub trait SyscallHandler<X: Xlen>: Send + Sync {
    /// Generate IR for an ECALL instruction.
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X>;
}

/// Syscall action for a syscall table entry.
#[derive(Clone, Copy, Debug)]
pub enum SyscallAction {
    /// Exit the program with a0 as exit code.
    Exit,
    /// Call a runtime function with arguments a0..a5.
    Runtime { name: &'static str, args: u8 },
    /// Return a fixed value in a0 (may be negative).
    ReturnConst(i64),
}

/// Syscall table entry.
#[derive(Clone, Copy, Debug)]
pub struct SyscallEntry {
    pub num: u64,
    pub action: SyscallAction,
}

impl SyscallEntry {
    pub const fn exit(num: u64) -> Self {
        Self {
            num,
            action: SyscallAction::Exit,
        }
    }

    pub const fn runtime(num: u64, name: &'static str, args: u8) -> Self {
        Self {
            num,
            action: SyscallAction::Runtime { name, args },
        }
    }

    pub const fn ret(num: u64, value: i64) -> Self {
        Self {
            num,
            action: SyscallAction::ReturnConst(value),
        }
    }
}

/// Table-driven syscall handler.
#[derive(Clone, Debug)]
pub struct SyscallTable {
    abi: SyscallAbi,
    entries: Vec<SyscallEntry>,
    default_error: i64,
}

impl SyscallTable {
    /// Create a new syscall table with the given ABI.
    pub fn new(abi: SyscallAbi) -> Self {
        Self {
            abi,
            entries: Vec::new(),
            default_error: -38, // ENOSYS
        }
    }

    /// Add a syscall entry.
    pub fn with_entry(mut self, entry: SyscallEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Add an exit syscall entry.
    pub fn with_exit(self, num: u64) -> Self {
        self.with_entry(SyscallEntry::exit(num))
    }

    /// Add a runtime syscall entry.
    pub fn with_runtime(self, num: u64, name: &'static str, args: u8) -> Self {
        self.with_entry(SyscallEntry::runtime(num, name, args))
    }

    /// Add a fixed-return syscall entry.
    pub fn with_return(self, num: u64, value: i64) -> Self {
        self.with_entry(SyscallEntry::ret(num, value))
    }

    /// Set the default error code for unknown syscalls (default: -38 = ENOSYS).
    pub fn default_error(mut self, code: i64) -> Self {
        self.default_error = code;
        self
    }

    pub(crate) fn build_ir<X: Xlen>(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        let sys_reg = self.abi.syscall_reg();
        let sys_num = Expr::read(sys_reg);
        let a7_eq = |num: u64| Expr::eq(sys_num.clone(), Expr::imm(X::from_u64(num)));

        let mut entries = self.entries.clone();
        entries.sort_by_key(|e| e.num);

        let mut exit_entries = Vec::new();
        let mut non_exit_entries = Vec::new();

        for entry in entries {
            match entry.action {
                SyscallAction::Exit => exit_entries.push(entry),
                _ => non_exit_entries.push(entry),
            }
        }

        let mut stmts = Vec::new();

        // Exit syscalls: set exit flag and exit code (a0)
        for entry in &exit_entries {
            stmts.push(Stmt::if_then(
                a7_eq(entry.num),
                vec![
                    Stmt::write_exited(Expr::imm(X::from_u64(1))),
                    Stmt::write_exit_code(Expr::read(REG_A0)),
                ],
            ));
        }

        // Default error return
        let mut dispatch: Stmt<X> = Stmt::write_reg(
            REG_A0,
            Expr::imm(X::from_u64(self.default_error as u64)),
        );

        // Build non-exit dispatch chain
        for entry in non_exit_entries.iter().rev() {
            dispatch = match entry.action {
                SyscallAction::Runtime { name, args } => {
                    let mut call_args = Vec::with_capacity((args as usize) + 1);
                    call_args.push(Expr::var("state"));
                    for i in 0..args {
                        call_args.push(Expr::read(REG_A0 + i));
                    }
                    Stmt::if_then_else(
                        a7_eq(entry.num),
                        vec![Stmt::write_reg(
                            REG_A0,
                            Expr::extern_call(name, call_args, (X::REG_BYTES * 8) as u8),
                        )],
                        vec![dispatch],
                    )
                }
                SyscallAction::ReturnConst(value) => Stmt::if_then_else(
                    a7_eq(entry.num),
                    vec![Stmt::write_reg(
                        REG_A0,
                        Expr::imm(X::from_u64(value as u64)),
                    )],
                    vec![dispatch],
                ),
                SyscallAction::Exit => dispatch,
            };
        }

        if !non_exit_entries.is_empty() {
            if !exit_entries.is_empty() {
                let mut is_exit = a7_eq(exit_entries[0].num);
                for entry in exit_entries.iter().skip(1) {
                    is_exit = Expr::or(is_exit, a7_eq(entry.num));
                }
                stmts.push(Stmt::if_then(Expr::not(is_exit), vec![dispatch]));
            } else {
                stmts.push(dispatch);
            }
        }

        let next_pc = instr.pc + X::Reg::from(instr.size as u32);
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            stmts,
            Terminator::fall(next_pc),
        )
    }
}

impl<X: Xlen> SyscallHandler<X> for SyscallTable {
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        self.build_ir(instr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodedInstr, InstrArgs, OP_ECALL};
    use rvr_ir::{Rv64, WriteTarget};

    fn make_ecall_instr() -> DecodedInstr<Rv64> {
        DecodedInstr {
            pc: 0x1000,
            opid: OP_ECALL,
            size: 4,
            args: InstrArgs::None,
        }
    }

    fn has_exit_write(stmts: &[Stmt<Rv64>]) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Write { target, .. } => match target {
                    WriteTarget::Exited | WriteTarget::ExitCode => return true,
                    _ => {}
                },
                Stmt::If {
                    then_stmts,
                    else_stmts,
                    ..
                } => {
                    if has_exit_write(then_stmts) || has_exit_write(else_stmts) {
                        return true;
                    }
                }
                Stmt::ExternCall { .. } => {}
            }
        }
        false
    }

    #[test]
    fn test_syscall_table_custom() {
        let handler = SyscallTable::new(SyscallAbi::Standard)
            .with_exit(93)
            .with_runtime(64, "rv_sys_write", 3)
            .with_return(200, -1);

        let instr = make_ecall_instr();
        let ir = handler.handle_ecall(&instr);

        assert!(matches!(ir.terminator, Terminator::Fall { .. }));
        assert!(!ir.statements.is_empty());
        assert!(has_exit_write(&ir.statements));
    }
}
