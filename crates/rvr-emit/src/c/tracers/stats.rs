//! Stats tracer header generation.

use rvr_ir::Xlen;
use rvr_isa::REG_ABI_NAMES;

use super::super::signature::reg_type;

pub fn gen_tracer_stats<X: Xlen>() -> String {
    let reg_names = REG_ABI_NAMES
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let rtype = reg_type::<X>();

    format!(
        include_str!("templates/stats.h.in"),
        rtype = rtype,
        reg_names = reg_names.as_str(),
    )
}
