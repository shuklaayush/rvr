//! Syscall/ECALL handling for RISC-V.
//!
//! Provides a small, table-driven mechanism for lowering ECALL to IR.
//! The default handler matches riscv-tests semantics (exit with a0).
//! Linux-style syscalls are handled via a syscall table that dispatches
//! to runtime C helpers (rv_sys_*).

//!
//! # Usage
//!
//! ```ignore
//! use rvr_isa::{ExtensionRegistry, Rv64};
//! use rvr_isa::syscalls::{LinuxHandler, SyscallAbi};
//!
//! let registry = ExtensionRegistry::<Rv64>::standard()
//!     .with_syscall_handler(LinuxHandler::new(SyscallAbi::Standard));
//! ```

use rvr_ir::{Expr, InstrIR, Stmt, Terminator, Xlen};

use crate::{DecodedInstr, REG_A0, REG_A7, REG_T0};

/// Known Linux syscall numbers (RISC-V ABI).
pub mod syscall_nr {
    pub const SYS_GETCWD: u64 = 17;
    pub const SYS_FCNTL: u64 = 25;
    pub const SYS_OPENAT: u64 = 56;
    pub const SYS_CLOSE: u64 = 57;
    pub const SYS_GETDENTS64: u64 = 61;
    pub const SYS_READ: u64 = 63;
    pub const SYS_WRITE: u64 = 64;
    pub const SYS_PREAD64: u64 = 67;
    pub const SYS_FSTAT: u64 = 80;
    pub const SYS_EXIT: u64 = 93;
    pub const SYS_EXIT_GROUP: u64 = 94;
    pub const SYS_SET_TID_ADDRESS: u64 = 96;
    pub const SYS_SETPRIORITY: u64 = 99;
    pub const SYS_SCHED_SETSCHEDULER: u64 = 119;
    pub const SYS_SCHED_GETSCHEDULER: u64 = 120;
    pub const SYS_SCHED_GETPARAM: u64 = 121;
    pub const SYS_SCHED_GET_PRIORITY_MAX: u64 = 125;
    pub const SYS_SCHED_GET_PRIORITY_MIN: u64 = 126;
    pub const SYS_TGKILL: u64 = 131;
    pub const SYS_GETPID: u64 = 172;
    pub const SYS_GETTID: u64 = 178;
    pub const SYS_SYSINFO: u64 = 179;
    pub const SYS_BRK: u64 = 214;
    pub const SYS_MUNMAP: u64 = 215;
    pub const SYS_MREMAP: u64 = 216;
    pub const SYS_MMAP: u64 = 222;
    pub const SYS_MPROTECT: u64 = 226;
    pub const SYS_MADVISE: u64 = 233;
    pub const SYS_RISCV_HWPROBE: u64 = 258;
    pub const SYS_PRLIMIT64: u64 = 261;
    pub const SYS_GETRANDOM: u64 = 278;
    pub const SYS_RSEQ: u64 = 293;
    pub const SYS_CLOCK_GETTIME: u64 = 113;
    pub const SYS_CLOCK_GETTIME64: u64 = 403;
}

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
///
/// Implement this trait to customize syscall behavior in the recompiler.
pub trait SyscallHandler<X: Xlen>: Send + Sync {
    /// Generate IR for an ECALL instruction.
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X>;
}

/// Minimal handler for riscv-tests.
///
/// Treats ECALL as exit with a0 as the exit code.
#[derive(Debug, Clone, Copy, Default)]
pub struct RiscvTestsHandler;

impl<X: Xlen> SyscallHandler<X> for RiscvTestsHandler {
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            Vec::new(),
            Terminator::exit(Expr::read(REG_A0)),
        )
    }
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

    /// Create a Linux-compatible syscall table (standard ABI).
    pub fn linux() -> Self {
        Self::linux_with_abi(SyscallAbi::Standard)
    }

    /// Create a Linux-compatible syscall table with a specific ABI.
    pub fn linux_with_abi(abi: SyscallAbi) -> Self {
        use syscall_nr::*;
        Self::new(abi)
            .with_exit(SYS_EXIT)
            .with_exit(SYS_EXIT_GROUP)
            .with_runtime(SYS_WRITE, "rv_sys_write", 3)
            .with_runtime(SYS_READ, "rv_sys_read", 3)
            .with_runtime(SYS_BRK, "rv_sys_brk", 1)
            .with_runtime(SYS_MMAP, "rv_sys_mmap", 6)
            .with_runtime(SYS_FSTAT, "rv_sys_fstat", 2)
            .with_runtime(SYS_GETRANDOM, "rv_sys_getrandom", 3)
            .with_runtime(SYS_CLOCK_GETTIME, "rv_sys_clock_gettime", 2)
            .with_runtime(SYS_CLOCK_GETTIME64, "rv_sys_clock_gettime", 2)
            .with_return(SYS_RISCV_HWPROBE, -38)
            .with_return(SYS_RSEQ, -38)
            .with_return(SYS_SET_TID_ADDRESS, 1)
            .with_return(SYS_GETPID, 1)
            .with_return(SYS_GETTID, 1)
            .with_return(SYS_SCHED_GET_PRIORITY_MAX, 99)
            .with_return(SYS_SCHED_GET_PRIORITY_MIN, 1)
            .with_return(SYS_GETCWD, -1)
            .with_return(SYS_OPENAT, -1)
            .with_return(SYS_SYSINFO, -1)
            .with_return(SYS_MREMAP, -1)
            .with_return(SYS_FCNTL, -1)
            .with_return(SYS_CLOSE, 0)
            .with_return(SYS_GETDENTS64, -1)
            .with_return(SYS_PREAD64, -1)
            .with_return(SYS_SETPRIORITY, -1)
            .with_return(SYS_SCHED_SETSCHEDULER, -1)
            .with_return(SYS_SCHED_GETSCHEDULER, -1)
            .with_return(SYS_SCHED_GETPARAM, -1)
            .with_return(SYS_TGKILL, -1)
            .with_return(SYS_MUNMAP, 0)
            .with_return(SYS_MPROTECT, 0)
            .with_return(SYS_MADVISE, 0)
            .with_return(SYS_PRLIMIT64, -1)
    }

    fn build_ir<X: Xlen>(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
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

/// Linux syscall handler using a default syscall table.
#[derive(Clone, Debug)]
pub struct LinuxHandler {
    table: SyscallTable,
}

impl LinuxHandler {
    pub fn new(abi: SyscallAbi) -> Self {
        Self {
            table: SyscallTable::linux_with_abi(abi),
        }
    }
}

impl Default for LinuxHandler {
    fn default() -> Self {
        Self::new(SyscallAbi::Standard)
    }
}

impl<X: Xlen> SyscallHandler<X> for LinuxHandler {
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        self.table.handle_ecall(instr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rvr_ir::WriteTarget;
    use crate::{DecodedInstr, InstrArgs, OP_ECALL};
    use rvr_ir::Rv64;

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
    fn test_riscv_tests_handler() {
        let handler = RiscvTestsHandler;
        let instr = make_ecall_instr();
        let ir = handler.handle_ecall(&instr);

        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }

    #[test]
    fn test_linux_handler() {
        let handler = LinuxHandler::default();
        let instr = make_ecall_instr();
        let ir = handler.handle_ecall(&instr);

        assert!(matches!(ir.terminator, Terminator::Fall { .. }));
        assert!(!ir.statements.is_empty());
        assert!(has_exit_write(&ir.statements));
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
    }
}
