//! Expression IR.

use crate::xlen::Xlen;

/// Unary operations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    Not,
    Neg,
    Sext8,
    Sext16,
    Sext32,
    Zext8,
    Zext16,
    Zext32,
    Clz,
    Ctz,
    Cpop,
    Clz32,
    Ctz32,
    Cpop32,
    Orc8,
    Rev8,
    Brev8,
    Zip,
    Unzip,
}

/// Binary operations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    MulH,
    MulHSU,
    MulHU,
    Div,
    DivU,
    Rem,
    RemU,
    And,
    Or,
    Xor,
    Sll,
    Srl,
    Sra,
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,
    AddW,
    SubW,
    MulW,
    DivW,
    DivUW,
    RemW,
    RemUW,
    SllW,
    SrlW,
    SraW,
    Pack,
    Pack8,
    Pack16,
}

/// Ternary operations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TernaryOp {
    Select,
}

/// Read expressions.
#[derive(Clone, Debug)]
pub enum ReadExpr<X: Xlen> {
    Reg(u8),
    Csr(u16),
    Mem {
        base: Box<Expr<X>>,
        offset: i16,
        width: u8,
        signed: bool,
    },
    MemAddr {
        addr: Box<Expr<X>>,
        width: u8,
        signed: bool,
    },
    Pc,
    Cycle,
    Instret,
    Temp(u8),
    TraceIdx,
    PcIdx,
    ResAddr,
    ResValid,
    Exited,
    ExitCode,
}

/// Expression tree node.
#[derive(Clone, Debug)]
pub enum Expr<X: Xlen> {
    Imm(X::Reg),
    Read(ReadExpr<X>),
    PcConst(X::Reg),
    Var(String),
    Unary {
        op: UnaryOp,
        expr: Box<Self>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Self>,
        right: Box<Self>,
    },
    Ternary {
        op: TernaryOp,
        first: Box<Self>,
        second: Box<Self>,
        third: Box<Self>,
    },
    ExternCall {
        name: String,
        args: Vec<Self>,
        ret_width: u8,
    },
}

// These are factory methods, not trait implementations
#[allow(clippy::should_implement_trait)]
impl<X: Xlen> Expr<X> {
    /// Create an immediate expression.
    pub const fn imm(val: X::Reg) -> Self {
        Self::Imm(val)
    }

    /// Create a register read expression.
    #[must_use]
    pub const fn reg(idx: u8) -> Self {
        Self::Read(ReadExpr::Reg(idx))
    }

    /// Create a memory read expression.
    pub fn mem(base: Self, offset: i16, width: u8, signed: bool) -> Self {
        Self::Read(ReadExpr::Mem {
            base: Box::new(base),
            offset,
            width,
            signed,
        })
    }

    /// Create a memory read expression with computed address.
    pub fn mem_addr(addr: Self, width: u8, signed: bool) -> Self {
        Self::Read(ReadExpr::MemAddr {
            addr: Box::new(addr),
            width,
            signed,
        })
    }

    /// Create a CSR read expression.
    #[must_use]
    pub const fn csr(csr: u16) -> Self {
        Self::Read(ReadExpr::Csr(csr))
    }

    /// Create a PC constant expression.
    pub const fn pc_const(pc: X::Reg) -> Self {
        Self::PcConst(pc)
    }

    /// Create a C variable reference.
    #[must_use]
    pub fn var(name: &str) -> Self {
        Self::Var(name.to_string())
    }

    /// Create an external function call expression.
    #[must_use]
    pub fn extern_call(fn_name: &str, args: Vec<Self>, ret_width: u8) -> Self {
        Self::ExternCall {
            name: fn_name.to_string(),
            args,
            ret_width,
        }
    }

    fn unary(op: UnaryOp, expr: Self) -> Self {
        Self::Unary {
            op,
            expr: Box::new(expr),
        }
    }

    fn binary(op: BinaryOp, left: Self, right: Self) -> Self {
        Self::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Check if expression is statically known to be zero.
    /// This includes Imm(0) and Read(Reg(0)) since x0 is hardwired to zero.
    fn is_zero(&self) -> bool {
        match self {
            Self::Imm(v) => X::to_u64(*v) == 0,
            Self::Read(ReadExpr::Reg(0)) => true,
            _ => false,
        }
    }

    fn ternary(op: TernaryOp, first: Self, second: Self, third: Self) -> Self {
        Self::Ternary {
            op,
            first: Box::new(first),
            second: Box::new(second),
            third: Box::new(third),
        }
    }

    pub fn add(left: Self, right: Self) -> Self {
        // Constant fold and identity optimizations
        // Handles: add(imm, imm), add(x, 0), add(0, x), add(reg(0), x), add(x, reg(0))
        if let (Self::Imm(l), Self::Imm(r)) = (&left, &right) {
            return Self::Imm(X::from_u64(X::to_u64(*l).wrapping_add(X::to_u64(*r))));
        }
        if right.is_zero() {
            return left;
        }
        if left.is_zero() {
            return right;
        }
        Self::binary(BinaryOp::Add, left, right)
    }

    pub fn sub(left: Self, right: Self) -> Self {
        // Handles: sub(imm, imm), sub(x, 0), sub(0, x) -> neg(x)
        if let (Self::Imm(l), Self::Imm(r)) = (&left, &right) {
            return Self::Imm(X::from_u64(X::to_u64(*l).wrapping_sub(X::to_u64(*r))));
        }
        if right.is_zero() {
            return left;
        }
        if left.is_zero() {
            return Self::neg(right);
        }
        Self::binary(BinaryOp::Sub, left, right)
    }

    pub fn mul(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Mul, left, right)
    }

    pub fn and(left: Self, right: Self) -> Self {
        if let (Self::Imm(l), Self::Imm(r)) = (&left, &right) {
            return Self::Imm(X::from_u64(X::to_u64(*l) & X::to_u64(*r)));
        }
        Self::binary(BinaryOp::And, left, right)
    }

    pub fn or(left: Self, right: Self) -> Self {
        if let (Self::Imm(l), Self::Imm(r)) = (&left, &right) {
            return Self::Imm(X::from_u64(X::to_u64(*l) | X::to_u64(*r)));
        }
        if right.is_zero() {
            return left;
        }
        if left.is_zero() {
            return right;
        }
        Self::binary(BinaryOp::Or, left, right)
    }

    pub fn xor(left: Self, right: Self) -> Self {
        if let (Self::Imm(l), Self::Imm(r)) = (&left, &right) {
            return Self::Imm(X::from_u64(X::to_u64(*l) ^ X::to_u64(*r)));
        }
        if right.is_zero() {
            return left;
        }
        if left.is_zero() {
            return right;
        }
        Self::binary(BinaryOp::Xor, left, right)
    }

    pub fn sll(left: Self, right: Self) -> Self {
        if right.is_zero() {
            return left;
        }
        Self::binary(BinaryOp::Sll, left, right)
    }

    pub fn srl(left: Self, right: Self) -> Self {
        if right.is_zero() {
            return left;
        }
        Self::binary(BinaryOp::Srl, left, right)
    }

    pub fn sra(left: Self, right: Self) -> Self {
        if right.is_zero() {
            return left;
        }
        Self::binary(BinaryOp::Sra, left, right)
    }

    pub fn eq(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Eq, left, right)
    }

    pub fn ne(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Ne, left, right)
    }

    pub fn lt(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Lt, left, right)
    }

    pub fn ge(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Ge, left, right)
    }

    pub fn ltu(left: Self, right: Self) -> Self {
        // sltu(0, x) is (0 < x) which equals (x != 0)
        if left.is_zero() {
            return Self::ne(right, Self::Imm(X::from_u64(0)));
        }
        Self::binary(BinaryOp::Ltu, left, right)
    }

    pub fn geu(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Geu, left, right)
    }

    /// Create a ternary select (cond ? then : else).
    pub fn select(cond: Self, then_val: Self, else_val: Self) -> Self {
        Self::ternary(TernaryOp::Select, cond, then_val, else_val)
    }

    /// Sign extend from 32 bits.
    pub fn sext32(val: Self) -> Self {
        // Constant fold: sext32(imm) -> imm with sign extension applied
        if let Self::Imm(v) = &val {
            let v64 = X::to_u64(*v);
            let low = u32::try_from(v64 & 0xffff_ffff).unwrap_or(0);
            let sext = i64::from(low.cast_signed()).cast_unsigned();
            return Self::Imm(X::from_u64(sext));
        }
        Self::unary(UnaryOp::Sext32, val)
    }

    /// Create a NOT expression.
    pub fn not(val: Self) -> Self {
        Self::unary(UnaryOp::Not, val)
    }

    // ===== Register/Memory shorthand =====

    /// Create a register read expression (alias for reg).
    #[must_use]
    pub const fn read(idx: u8) -> Self {
        Self::reg(idx)
    }

    /// Create a memory read with computed address (unsigned).
    pub fn mem_u(addr: Self, width: u8) -> Self {
        Self::mem_addr(addr, width, false)
    }

    /// Create a signed memory read with computed address.
    pub fn mem_s(addr: Self, width: u8) -> Self {
        Self::mem_addr(addr, width, true)
    }

    // ===== Arithmetic ops =====

    pub fn div(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Div, left, right)
    }

    pub fn divu(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::DivU, left, right)
    }

    pub fn rem(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Rem, left, right)
    }

    pub fn remu(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::RemU, left, right)
    }

    // ===== RV64 word ops =====

    pub fn addw(left: Self, right: Self) -> Self {
        // addw(x, 0) or addw(0, x) is just sext32(x)
        if right.is_zero() {
            return Self::sext32(left);
        }
        if left.is_zero() {
            return Self::sext32(right);
        }
        Self::binary(BinaryOp::AddW, left, right)
    }

    pub fn subw(left: Self, right: Self) -> Self {
        // subw(x, 0) is sext32(x)
        if right.is_zero() {
            return Self::sext32(left);
        }
        // subw(0, x) is negw: sext32(neg(x))
        if left.is_zero() {
            return Self::sext32(Self::neg(right));
        }
        Self::binary(BinaryOp::SubW, left, right)
    }

    pub fn sllw(left: Self, right: Self) -> Self {
        if right.is_zero() {
            return Self::sext32(left);
        }
        Self::binary(BinaryOp::SllW, left, right)
    }

    pub fn srlw(left: Self, right: Self) -> Self {
        if right.is_zero() {
            return Self::sext32(left);
        }
        Self::binary(BinaryOp::SrlW, left, right)
    }

    pub fn sraw(left: Self, right: Self) -> Self {
        if right.is_zero() {
            return Self::sext32(left);
        }
        Self::binary(BinaryOp::SraW, left, right)
    }

    pub fn mulw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::MulW, left, right)
    }

    pub fn divw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::DivW, left, right)
    }

    pub fn divuw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::DivUW, left, right)
    }

    pub fn remw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::RemW, left, right)
    }

    pub fn remuw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::RemUW, left, right)
    }

    // ===== Mul high =====

    pub fn mulh(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::MulH, left, right)
    }

    pub fn mulhsu(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::MulHSU, left, right)
    }

    pub fn mulhu(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::MulHU, left, right)
    }

    // ===== Unary ops =====

    pub fn neg(val: Self) -> Self {
        Self::unary(UnaryOp::Neg, val)
    }

    pub fn clz(val: Self) -> Self {
        Self::unary(UnaryOp::Clz, val)
    }

    pub fn ctz(val: Self) -> Self {
        Self::unary(UnaryOp::Ctz, val)
    }

    pub fn cpop(val: Self) -> Self {
        Self::unary(UnaryOp::Cpop, val)
    }

    pub fn clz32(val: Self) -> Self {
        Self::unary(UnaryOp::Clz32, val)
    }

    pub fn ctz32(val: Self) -> Self {
        Self::unary(UnaryOp::Ctz32, val)
    }

    pub fn cpop32(val: Self) -> Self {
        Self::unary(UnaryOp::Cpop32, val)
    }

    pub fn orc8(val: Self) -> Self {
        Self::unary(UnaryOp::Orc8, val)
    }

    pub fn rev8(val: Self) -> Self {
        Self::unary(UnaryOp::Rev8, val)
    }

    pub fn sext8(val: Self) -> Self {
        if let Self::Imm(v) = &val {
            let low = u8::try_from(X::to_u64(*v) & 0xff).unwrap_or(0);
            let sext = i64::from(i8::from_le_bytes([low])).cast_unsigned();
            return Self::Imm(X::from_u64(sext));
        }
        Self::unary(UnaryOp::Sext8, val)
    }

    pub fn sext16(val: Self) -> Self {
        if let Self::Imm(v) = &val {
            let low = u16::try_from(X::to_u64(*v) & 0xffff).unwrap_or(0);
            let sext = i64::from(i16::from_le_bytes(low.to_le_bytes())).cast_unsigned();
            return Self::Imm(X::from_u64(sext));
        }
        Self::unary(UnaryOp::Sext16, val)
    }

    pub fn zext8(val: Self) -> Self {
        if let Self::Imm(v) = &val {
            return Self::Imm(X::from_u64(X::to_u64(*v) & 0xff));
        }
        Self::unary(UnaryOp::Zext8, val)
    }

    pub fn zext16(val: Self) -> Self {
        if let Self::Imm(v) = &val {
            return Self::Imm(X::from_u64(X::to_u64(*v) & 0xffff));
        }
        Self::unary(UnaryOp::Zext16, val)
    }

    pub fn zext32(val: Self) -> Self {
        if let Self::Imm(v) = &val {
            return Self::Imm(X::from_u64(X::to_u64(*v) & 0xffff_ffff));
        }
        Self::unary(UnaryOp::Zext32, val)
    }

    // ===== Zbkb =====

    pub fn pack(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Pack, left, right)
    }

    pub fn pack8(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Pack8, left, right)
    }

    pub fn pack16(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Pack16, left, right)
    }

    pub fn brev8(val: Self) -> Self {
        Self::unary(UnaryOp::Brev8, val)
    }

    pub fn zip(val: Self) -> Self {
        Self::unary(UnaryOp::Zip, val)
    }

    pub fn unzip(val: Self) -> Self {
        Self::unary(UnaryOp::Unzip, val)
    }

    // ===== Special reads =====

    #[must_use]
    pub const fn res_addr() -> Self {
        Self::Read(ReadExpr::ResAddr)
    }

    #[must_use]
    pub const fn res_valid() -> Self {
        Self::Read(ReadExpr::ResValid)
    }

    #[must_use]
    pub const fn instret() -> Self {
        Self::Read(ReadExpr::Instret)
    }

    #[must_use]
    pub const fn cycle() -> Self {
        Self::Read(ReadExpr::Cycle)
    }

    #[must_use]
    pub const fn temp(idx: u8) -> Self {
        Self::Read(ReadExpr::Temp(idx))
    }

    #[must_use]
    pub const fn trace_idx() -> Self {
        Self::Read(ReadExpr::TraceIdx)
    }

    #[must_use]
    pub const fn pc_idx() -> Self {
        Self::Read(ReadExpr::PcIdx)
    }

    #[must_use]
    pub const fn exited() -> Self {
        Self::Read(ReadExpr::Exited)
    }

    #[must_use]
    pub const fn exit_code() -> Self {
        Self::Read(ReadExpr::ExitCode)
    }

    // ===== Comparison shortcuts =====

    pub fn slt(left: Self, right: Self) -> Self {
        Self::lt(left, right)
    }

    pub fn sltu(left: Self, right: Self) -> Self {
        Self::ltu(left, right)
    }

    // ===== AMO min/max operations =====

    pub fn min(left: Self, right: Self) -> Self {
        let cond = Self::lt(left.clone(), right.clone());
        Self::select(cond, left, right)
    }

    pub fn max(left: Self, right: Self) -> Self {
        let cond = Self::lt(left.clone(), right.clone());
        Self::select(cond, right, left)
    }

    pub fn minu(left: Self, right: Self) -> Self {
        let cond = Self::ltu(left.clone(), right.clone());
        Self::select(cond, left, right)
    }

    pub fn maxu(left: Self, right: Self) -> Self {
        let cond = Self::ltu(left.clone(), right.clone());
        Self::select(cond, right, left)
    }

    // ===== 32-bit AMO min/max operations (for .w variants) =====

    /// Min of two values compared as signed 32-bit integers.
    pub fn min32(left: Self, right: Self) -> Self {
        // Truncate both to 32 bits, sign-extend, then compare
        let l32 = Self::sext32(left.clone());
        let r32 = Self::sext32(right.clone());
        let cond = Self::lt(l32, r32);
        Self::select(cond, left, right)
    }

    /// Max of two values compared as signed 32-bit integers.
    pub fn max32(left: Self, right: Self) -> Self {
        let l32 = Self::sext32(left.clone());
        let r32 = Self::sext32(right.clone());
        let cond = Self::lt(l32, r32);
        Self::select(cond, right, left)
    }

    /// Min of two values compared as unsigned 32-bit integers.
    pub fn minu32(left: Self, right: Self) -> Self {
        // Truncate both to 32 bits, zero-extend, then compare unsigned
        let l32 = Self::zext32(left.clone());
        let r32 = Self::zext32(right.clone());
        let cond = Self::ltu(l32, r32);
        Self::select(cond, left, right)
    }

    /// Max of two values compared as unsigned 32-bit integers.
    pub fn maxu32(left: Self, right: Self) -> Self {
        let l32 = Self::zext32(left.clone());
        let r32 = Self::zext32(right.clone());
        let cond = Self::ltu(l32, r32);
        Self::select(cond, right, left)
    }
}

#[cfg(test)]
mod tests;
