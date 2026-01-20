//! Syscall/ECALL handling for RISC-V.
//!
//! Provides a trait-based architecture for customizing ECALL behavior.
//! The default implementation follows riscv-tests semantics (exit with a0).
//!
//! # Usage
//!
//! Use the override mechanism to swap ECALL behavior:
//!
//! ```ignore
//! use rvr_isa::{ExtensionRegistry, OP_ECALL};
//! use rvr_isa::syscalls::{SyscallOverride, LinuxHandler};
//!
//! let registry = ExtensionRegistry::<Rv64>::standard()
//!     .with_override(OP_ECALL, SyscallOverride::new(LinuxHandler));
//! ```

use rvr_ir::{Expr, InstrIR, Terminator, Xlen};

use crate::{DecodedInstr, InstructionOverride};

/// Register indices for syscall convention.
pub const REG_A0: u8 = 10;
pub const REG_A1: u8 = 11;
pub const REG_A2: u8 = 12;
pub const REG_A3: u8 = 13;
pub const REG_A4: u8 = 14;
pub const REG_A5: u8 = 15;
pub const REG_A6: u8 = 16;
pub const REG_A7: u8 = 17;

/// Known Linux syscall numbers (RISC-V ABI).
pub mod syscall_nr {
    pub const SYS_EXIT: u64 = 93;
    pub const SYS_EXIT_GROUP: u64 = 94;
    pub const SYS_WRITE: u64 = 64;
    pub const SYS_READ: u64 = 63;
    pub const SYS_BRK: u64 = 214;
    pub const SYS_MMAP: u64 = 222;
    pub const SYS_FSTAT: u64 = 80;
    pub const SYS_GETRANDOM: u64 = 278;
    pub const SYS_CLOCK_GETTIME: u64 = 113;
}

/// Trait for handling ECALL instructions.
///
/// Implement this trait to customize syscall behavior in the recompiler.
pub trait SyscallHandler<X: Xlen>: Send + Sync {
    /// Generate IR for an ECALL instruction.
    ///
    /// # Arguments
    /// * `instr` - The decoded ECALL instruction
    ///
    /// # Returns
    /// The IR representing the syscall behavior.
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X>;
}

/// Minimal handler for riscv-tests.
///
/// Treats ECALL as exit with a0 as the exit code.
/// This is the default behavior and matches riscv-tests semantics.
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

/// Linux syscall handler stub.
///
/// Provides pattern for handling common Linux syscalls.
/// Currently implements exit/exit_group; other syscalls are TODOs.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinuxHandler;

impl<X: Xlen> SyscallHandler<X> for LinuxHandler {
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        // Linux syscall convention:
        // a7 = syscall number
        // a0-a5 = arguments
        // a0 = return value
        //
        // For static recompilation, we generate a runtime dispatch.
        // The generated C code will call into a syscall handler.
        //
        // For now, we generate an exit terminator that uses a7 to
        // encode the syscall number. The runtime can inspect this.
        //
        // TODO: Implement full syscall dispatch:
        // - SYS_WRITE: write(fd, buf, count)
        // - SYS_READ: read(fd, buf, count)
        // - SYS_BRK: brk(addr)
        // - SYS_MMAP: mmap(addr, len, prot, flags, fd, off)
        // - SYS_FSTAT: fstat(fd, statbuf)
        // - SYS_GETRANDOM: getrandom(buf, buflen, flags)
        // - SYS_CLOCK_GETTIME: clock_gettime(clk_id, tp)

        // For now, treat all syscalls as exit with a0 as code.
        // Runtime can check a7 for syscall number if needed.
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            Vec::new(),
            Terminator::exit(Expr::read(REG_A0)),
        )
    }
}

/// Adapter to use a SyscallHandler as an InstructionOverride.
///
/// This allows registering a syscall handler with the extension registry.
pub struct SyscallOverride<X: Xlen, H: SyscallHandler<X>> {
    handler: H,
    _phantom: std::marker::PhantomData<X>,
}

impl<X: Xlen, H: SyscallHandler<X>> SyscallOverride<X, H> {
    /// Create a new syscall override with the given handler.
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<X: Xlen, H: SyscallHandler<X>> InstructionOverride<X> for SyscallOverride<X, H> {
    fn lift(
        &self,
        instr: &DecodedInstr<X>,
        _default: &dyn Fn(&DecodedInstr<X>) -> InstrIR<X>,
    ) -> InstrIR<X> {
        self.handler.handle_ecall(instr)
    }
}

/// Builder for creating custom syscall handlers.
///
/// Allows composing syscall behavior from individual handlers.
#[derive(Default)]
pub struct SyscallBuilder<X: Xlen> {
    fallback: Option<Box<dyn SyscallHandler<X>>>,
    _phantom: std::marker::PhantomData<X>,
}

impl<X: Xlen> SyscallBuilder<X> {
    /// Create a new syscall builder.
    pub fn new() -> Self {
        Self {
            fallback: None,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set the fallback handler for unrecognized syscalls.
    pub fn fallback(mut self, handler: impl SyscallHandler<X> + 'static) -> Self {
        self.fallback = Some(Box::new(handler));
        self
    }

    /// Build a composite syscall handler.
    ///
    /// Returns RiscvTestsHandler if no fallback was set.
    pub fn build(self) -> impl SyscallHandler<X> {
        CompositeHandler {
            fallback: self.fallback.unwrap_or_else(|| Box::new(RiscvTestsHandler)),
        }
    }
}

struct CompositeHandler<X: Xlen> {
    fallback: Box<dyn SyscallHandler<X>>,
}

impl<X: Xlen> SyscallHandler<X> for CompositeHandler<X> {
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        // For now, just use fallback
        // TODO: Add per-syscall dispatch based on a7
        self.fallback.handle_ecall(instr)
    }
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
            args: InstrArgs::None,
        }
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
        let handler = LinuxHandler;
        let instr = make_ecall_instr();
        let ir = handler.handle_ecall(&instr);

        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }

    #[test]
    fn test_syscall_override() {
        let override_handler = SyscallOverride::new(RiscvTestsHandler);
        let instr = make_ecall_instr();

        let ir = override_handler.lift(&instr, &|_| panic!("default should not be called"));
        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }

    #[test]
    fn test_syscall_builder() {
        let handler = SyscallBuilder::<Rv64>::new()
            .fallback(RiscvTestsHandler)
            .build();

        let instr = make_ecall_instr();
        let ir = handler.handle_ecall(&instr);
        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }
}
