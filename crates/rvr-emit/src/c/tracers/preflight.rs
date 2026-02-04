//! Preflight tracer header generation.

use rvr_ir::Xlen;

use super::super::signature::reg_type;

pub fn gen_tracer_preflight<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    let rbytes = X::REG_BYTES.to_string();

    format!(
        include_str!("templates/preflight.h.in"),
        rtype = rtype,
        rbytes = rbytes.as_str(),
    )
}
