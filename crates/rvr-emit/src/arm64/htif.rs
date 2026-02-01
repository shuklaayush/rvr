//! HTIF (Host-Target Interface) code generation for ARM64 assembly.
//!
//! Generates assembly to handle HTIF protocol used by riscv-tests for:
//! - Exit signaling (exit code via tohost)
//!
//! The HTIF protocol writes to the tohost address:
//! - If value & 1 == 1: exit with code = value >> 1
//! - Otherwise: syscall (currently just writes through)

use rvr_ir::Xlen;

use super::Arm64Emitter;
use super::registers::reserved;
use crate::htif::TOHOST_ADDR;

impl<X: Xlen> Arm64Emitter<X> {
    /// Emit HTIF tohost check for memory stores.
    ///
    /// Called after the guest address is computed in x0 and value is in x1.
    /// If the address matches TOHOST_ADDR and value indicates exit (LSB=1),
    /// sets has_exited and exit_code, then branches to asm_exit.
    ///
    /// Otherwise falls through to the normal store path.
    pub(super) fn emit_htif_check(&mut self) {
        let not_tohost_label = self.next_label("not_tohost");

        // Compare address with TOHOST_ADDR
        self.load_imm("x2", TOHOST_ADDR);
        self.emit("cmp x0, x2");
        self.emitf(format!("b.ne {not_tohost_label}"));

        // Check if exit request (value & 1 == 1)
        let not_exit_label = self.next_label("not_exit");
        self.emit("tst x1, #1");
        self.emitf(format!("b.eq {not_exit_label}"));

        // Exit: set has_exited=1, exit_code = value >> 1
        let has_exited = self.layout.offset_has_exited;
        let exit_code = self.layout.offset_exit_code;
        self.emit("lsr x2, x1, #1");
        self.emit("mov w0, #1");
        self.emitf(format!(
            "strb w0, [{}, #{}]",
            reserved::STATE_PTR,
            has_exited
        ));
        self.emitf(format!(
            "strb w2, [{}, #{}]",
            reserved::STATE_PTR,
            exit_code
        ));
        self.emit("b asm_exit");

        self.emit_label(&not_exit_label);
        // Syscall requests: still do the write (needed for HTIF ack)

        self.emit_label(&not_tohost_label);
    }
}
