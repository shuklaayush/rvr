//! Diff tracer header generation.
//!
//! Captures single-instruction state for differential testing.
//! Uses bounded memory (~64 bytes) - only stores the most recent instruction's effects.
//!
//! Captured state:
//! - PC and opcode
//! - Register write (rd, value)
//! - Memory access (addr, value, width, `is_write`)

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_diff<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    format!(include_str!("templates/diff.h.in"), rtype = rtype)
}
