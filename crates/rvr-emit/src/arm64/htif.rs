//! HTIF (Host-Target Interface) code generation for ARM64 assembly.
//!
//! Generates assembly to handle HTIF protocol used by riscv-tests for:
//! - Exit signaling (exit code via tohost)
//! - Syscall handling (via handle_tohost_write)
//!
//! The HTIF protocol writes to the tohost address:
//! - If value & 1 == 1: exit with code = value >> 1
//! - Otherwise: syscall (call handle_tohost_write)

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
    /// For syscall requests (LSB=0), calls handle_tohost_write and skips
    /// the normal store (the handler does any needed memory writes).
    ///
    /// Returns a label that the caller must emit after the store instruction.
    /// For non-tohost addresses, falls through to the normal store path.
    pub(super) fn emit_htif_check(&mut self) -> String {
        let not_tohost_label = self.next_label("not_tohost");
        let done_store_label = self.next_label("done_store");

        // Compare address with TOHOST_ADDR
        // TOHOST_ADDR fits in 32 bits; load into w2 (zero-extended) to reduce instruction count.
        self.load_imm("w2", TOHOST_ADDR as u32 as u64);
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
        // Syscall request: call handle_tohost_write(state, value)
        // x1 contains the value, need to preserve it for the call

        // Save hot regs to state before calling out
        self.save_hot_regs_to_state();

        // Set up arguments: x0 = state, x1 = value (already in x1)
        self.emitf(format!("mov x0, {}", reserved::STATE_PTR));
        // x1 already contains the value

        // Call handle_tohost_write
        self.emit("bl handle_tohost_write");

        // Restore hot regs from state
        self.restore_hot_regs_from_state();

        // Check if has_exited was set by the handler
        self.emitf(format!(
            "ldrb w0, [{}, #{}]",
            reserved::STATE_PTR,
            has_exited
        ));
        self.emit("cbnz w0, asm_exit");

        // Skip the normal store (handler did any needed writes)
        self.emitf(format!("b {done_store_label}"));

        self.emit_label(&not_tohost_label);

        done_store_label
    }
}
