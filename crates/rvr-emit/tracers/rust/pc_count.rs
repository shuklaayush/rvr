// Example Rust-side tracer for analysis.
//
// Use scripts/emit_tracer_header.py to emit a C header skeleton when you
// want an inline C tracer with the same hook surface.

pub const TRACER_NAME: &str = "pc_count";

#[derive(Default)]
pub struct PcCountTracer {
    pub pcs: u64,
}

impl PcCountTracer {
    pub fn trace_pc(&mut self, _pc: u64, _op: u16) {
        self.pcs = self.pcs.saturating_add(1);
    }
}
