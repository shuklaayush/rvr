//! Linux-style syscall table.

use rvr_ir::{InstrIR, Xlen};

use crate::DecodedInstr;

use super::table::{SyscallAbi, SyscallHandler, SyscallTable};

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

/// Linux syscall handler using a default syscall table.
#[derive(Clone, Debug)]
pub struct LinuxHandler {
    table: SyscallTable,
}

impl LinuxHandler {
    #[must_use]
    pub fn new(abi: SyscallAbi) -> Self {
        Self {
            table: linux_table(abi),
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

fn linux_table(abi: SyscallAbi) -> SyscallTable {
    use syscall_nr::{
        SYS_BRK, SYS_CLOCK_GETTIME, SYS_CLOCK_GETTIME64, SYS_CLOSE, SYS_EXIT, SYS_EXIT_GROUP,
        SYS_FCNTL, SYS_FSTAT, SYS_GETCWD, SYS_GETDENTS64, SYS_GETPID, SYS_GETRANDOM, SYS_GETTID,
        SYS_MADVISE, SYS_MMAP, SYS_MPROTECT, SYS_MREMAP, SYS_MUNMAP, SYS_OPENAT, SYS_PREAD64,
        SYS_PRLIMIT64, SYS_READ, SYS_RISCV_HWPROBE, SYS_RSEQ, SYS_SCHED_GET_PRIORITY_MAX,
        SYS_SCHED_GET_PRIORITY_MIN, SYS_SCHED_GETPARAM, SYS_SCHED_GETSCHEDULER,
        SYS_SCHED_SETSCHEDULER, SYS_SET_TID_ADDRESS, SYS_SETPRIORITY, SYS_SYSINFO, SYS_TGKILL,
        SYS_WRITE,
    };
    SyscallTable::new(abi)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecodedInstr, InstrArgs, OP_ECALL};
    use rvr_ir::Rv64;

    fn make_ecall_instr() -> DecodedInstr<Rv64> {
        DecodedInstr {
            pc: 0x1000,
            opid: OP_ECALL,
            size: 4,
            raw: 0,
            args: InstrArgs::None,
        }
    }

    #[test]
    fn test_linux_handler() {
        let handler = LinuxHandler::default();
        let instr = make_ecall_instr();
        let ir = handler.handle_ecall(&instr);

        assert!(matches!(ir.terminator, rvr_ir::Terminator::Fall { .. }));
        assert!(!ir.statements.is_empty());
    }
}
