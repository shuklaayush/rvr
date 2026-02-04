//! Spike-compatible tracer header generation.

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_spike<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    let is_rv64 = X::VALUE == 64;
    let pc_fmt = if is_rv64 { "%016lx" } else { "%08x" };
    let val_fmt = if is_rv64 { "%016lx" } else { "%08x" };
    let pc_cast = if is_rv64 {
        "(unsigned long)"
    } else {
        "(unsigned)"
    };
    let val_cast = if is_rv64 {
        "(unsigned long)"
    } else {
        "(unsigned)"
    };

    format!(
        include_str!("templates/spike.h.in"),
        rtype = rtype,
        pc_fmt = pc_fmt,
        val_fmt = val_fmt,
        pc_cast = pc_cast,
        val_cast = val_cast,
    )
}
