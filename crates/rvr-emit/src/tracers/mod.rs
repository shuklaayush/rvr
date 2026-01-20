//! Built-in tracer header generators.

use rvr_ir::Xlen;

use crate::tracer::TracerKind;

mod debug;
mod dynamic;
mod ffi;
mod none;
mod preflight;
mod stats;

pub fn gen_tracer_header<X: Xlen>(kind: TracerKind) -> String {
    match kind {
        TracerKind::None => none::gen_tracer_none::<X>(),
        TracerKind::Preflight => preflight::gen_tracer_preflight::<X>(),
        TracerKind::Stats => stats::gen_tracer_stats::<X>(),
        TracerKind::Ffi => ffi::gen_tracer_ffi::<X>(),
        TracerKind::Dynamic => dynamic::gen_tracer_dynamic::<X>(),
        TracerKind::Debug => debug::gen_tracer_debug::<X>(),
    }
}
