//! Bare-metal ECALL handling.
//!
//! Treat ECALL as exit with a0 as the exit code, matching riscv-tests.

use rvr_ir::{Expr, InstrIR, Terminator, Xlen};

use crate::{DecodedInstr, REG_A0};

use super::table::SyscallHandler;

/// Bare-metal handler (exit with a0).
#[derive(Debug, Clone, Copy, Default)]
pub struct BareMetalHandler;

impl<X: Xlen> SyscallHandler<X> for BareMetalHandler {
    fn handle_ecall(&self, instr: &DecodedInstr<X>) -> InstrIR<X> {
        InstrIR::new(
            instr.pc,
            instr.size,
            instr.opid.pack(),
            instr.raw,
            Vec::new(),
            Terminator::exit(Expr::read(REG_A0)),
        )
    }
}

/// Backwards-compatible name for riscv-tests behavior.
pub type RiscvTestsHandler = BareMetalHandler;

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
    fn test_bare_metal_handler() {
        let handler = BareMetalHandler;
        let instr = make_ecall_instr();
        let ir = handler.handle_ecall(&instr);

        assert!(matches!(ir.terminator, Terminator::Exit { .. }));
    }
}
