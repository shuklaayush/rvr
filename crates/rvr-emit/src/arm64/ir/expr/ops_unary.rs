use super::{Arm64Emitter, Expr, UnaryOp, Xlen};

impl<X: Xlen> Arm64Emitter<X> {
    pub(super) fn emit_unary_op(&mut self, op: UnaryOp, inner: &Expr<X>, dest: &str) -> String {
        let mut inner_reg = self.emit_expr(inner, dest);
        if X::VALUE == 32 && dest.starts_with('w') && inner_reg.starts_with('x') {
            inner_reg = Self::reg_32(&inner_reg);
        }
        if inner_reg != dest {
            self.emitf(format!("mov {dest}, {inner_reg}"));
        }

        match op {
            UnaryOp::Neg => {
                self.emitf(format!("neg {dest}, {dest}"));
            }
            UnaryOp::Not => {
                self.emitf(format!("mvn {dest}, {dest}"));
            }
            UnaryOp::Sext8 => {
                self.emitf(format!("sxtb {dest}, {}", Self::reg_32(dest)));
            }
            UnaryOp::Sext16 => {
                self.emitf(format!("sxth {dest}, {}", Self::reg_32(dest)));
            }
            UnaryOp::Sext32 => {
                let dest64 = Self::reg_64(dest);
                self.emitf(format!("sxtw {dest64}, {}", Self::reg_32(dest)));
            }
            UnaryOp::Zext8 => {
                self.emitf(format!(
                    "uxtb {}, {}",
                    Self::reg_32(dest),
                    Self::reg_32(dest)
                ));
            }
            UnaryOp::Zext16 => {
                self.emitf(format!(
                    "uxth {}, {}",
                    Self::reg_32(dest),
                    Self::reg_32(dest)
                ));
            }
            UnaryOp::Zext32 => {
                // Moving w to x zero-extends automatically
                let src32 = Self::reg_32(dest);
                let dest32 = Self::reg_32(dest);
                self.emitf(format!("mov {dest32}, {src32}"));
            }
            UnaryOp::Clz => {
                self.emitf(format!("clz {dest}, {dest}"));
            }
            UnaryOp::Ctz => {
                // ctz = clz(rbit(x))
                self.emitf(format!("rbit {dest}, {dest}"));
                self.emitf(format!("clz {dest}, {dest}"));
            }
            UnaryOp::Cpop => {
                if X::VALUE == 32 {
                    let dest32 = Self::reg_32(dest);
                    self.emit_cpop32(&dest32);
                } else {
                    self.emit_cpop64(dest);
                }
            }
            UnaryOp::Clz32 => {
                let dest32 = Self::reg_32(dest);
                self.emitf(format!("clz {dest32}, {dest32}"));
            }
            UnaryOp::Ctz32 => {
                let dest32 = Self::reg_32(dest);
                self.emitf(format!("rbit {dest32}, {dest32}"));
                self.emitf(format!("clz {dest32}, {dest32}"));
            }
            UnaryOp::Cpop32 => {
                let dest32 = Self::reg_32(dest);
                self.emit_cpop32(&dest32);
            }
            UnaryOp::Orc8 => {
                if X::VALUE == 32 {
                    let dest32 = Self::reg_32(dest);
                    self.emit_orc8_32(&dest32);
                } else {
                    self.emit_orc8_64(dest);
                }
            }
            UnaryOp::Rev8 => {
                // Byte reverse
                self.emitf(format!("rev {dest}, {dest}"));
            }
            _ => {
                self.emit_comment(&format!("unary op {op:?} not implemented"));
                self.emitf(format!("mov {dest}, {dest}"));
            }
        }

        dest.to_string()
    }

    fn emit_cpop64(&mut self, dest: &str) {
        let tmp = Self::temp3();
        self.emitf(format!("lsr {tmp}, {dest}, #1"));
        self.emitf(format!("and {tmp}, {tmp}, #0x5555555555555555"));
        self.emitf(format!("sub {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #2"));
        self.emitf(format!("and {tmp}, {tmp}, #0x3333333333333333"));
        self.emitf(format!("and {dest}, {dest}, #0x3333333333333333"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #4"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("and {dest}, {dest}, #0x0f0f0f0f0f0f0f0f"));
        self.emitf(format!("lsr {tmp}, {dest}, #8"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #16"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #32"));
        self.emitf(format!("add {dest}, {dest}, {tmp}"));
        self.emitf(format!("and {dest}, {dest}, #0x7f"));
    }

    fn emit_cpop32(&mut self, dest32: &str) {
        let tmp = Self::temp3();
        let tmp32 = Self::reg_32(tmp);
        self.emitf(format!("lsr {tmp32}, {dest32}, #1"));
        self.emitf(format!("and {tmp32}, {tmp32}, #0x55555555"));
        self.emitf(format!("sub {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #2"));
        self.emitf(format!("and {tmp32}, {tmp32}, #0x33333333"));
        self.emitf(format!("and {dest32}, {dest32}, #0x33333333"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #4"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("and {dest32}, {dest32}, #0x0f0f0f0f"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #8"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #16"));
        self.emitf(format!("add {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("and {dest32}, {dest32}, #0x3f"));
    }

    fn emit_orc8_64(&mut self, dest: &str) {
        let tmp = Self::temp3();
        self.emitf(format!("lsr {tmp}, {dest}, #1"));
        self.emitf(format!("orr {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #2"));
        self.emitf(format!("orr {dest}, {dest}, {tmp}"));
        self.emitf(format!("lsr {tmp}, {dest}, #4"));
        self.emitf(format!("orr {dest}, {dest}, {tmp}"));
        self.emitf(format!("and {dest}, {dest}, #0x0101010101010101"));
        self.emitf(format!("mov {tmp}, #0xff"));
        self.emitf(format!("mul {dest}, {dest}, {tmp}"));
    }

    fn emit_orc8_32(&mut self, dest32: &str) {
        let tmp = Self::temp3();
        let tmp32 = Self::reg_32(tmp);
        self.emitf(format!("lsr {tmp32}, {dest32}, #1"));
        self.emitf(format!("orr {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #2"));
        self.emitf(format!("orr {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("lsr {tmp32}, {dest32}, #4"));
        self.emitf(format!("orr {dest32}, {dest32}, {tmp32}"));
        self.emitf(format!("and {dest32}, {dest32}, #0x01010101"));
        self.emitf(format!("mov {tmp32}, #0xff"));
        self.emitf(format!("mul {dest32}, {dest32}, {tmp32}"));
    }
}
