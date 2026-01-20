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
        expr: Box<Expr<X>>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr<X>>,
        right: Box<Expr<X>>,
    },
    Ternary {
        op: TernaryOp,
        first: Box<Expr<X>>,
        second: Box<Expr<X>>,
        third: Box<Expr<X>>,
    },
    ExternCall {
        name: String,
        args: Vec<Expr<X>>,
        ret_width: u8,
    },
}

impl<X: Xlen> Expr<X> {
    /// Create an immediate expression.
    pub fn imm(val: X::Reg) -> Self {
        Self::Imm(val)
    }

    /// Create a register read expression.
    pub fn reg(idx: u8) -> Self {
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
    pub fn csr(csr: u16) -> Self {
        Self::Read(ReadExpr::Csr(csr))
    }

    /// Create a PC constant expression.
    pub fn pc_const(pc: X::Reg) -> Self {
        Self::PcConst(pc)
    }

    /// Create a C variable reference.
    pub fn var(name: &str) -> Self {
        Self::Var(name.to_string())
    }

    /// Create an external function call expression.
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

    fn ternary(op: TernaryOp, first: Self, second: Self, third: Self) -> Self {
        Self::Ternary {
            op,
            first: Box::new(first),
            second: Box::new(second),
            third: Box::new(third),
        }
    }

    pub fn add(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Add, left, right)
    }

    pub fn sub(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Sub, left, right)
    }

    pub fn mul(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Mul, left, right)
    }

    pub fn and(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::And, left, right)
    }

    pub fn or(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Or, left, right)
    }

    pub fn xor(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Xor, left, right)
    }

    pub fn sll(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Sll, left, right)
    }

    pub fn srl(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::Srl, left, right)
    }

    pub fn sra(left: Self, right: Self) -> Self {
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
        Self::unary(UnaryOp::Sext32, val)
    }

    /// Create a NOT expression.
    pub fn not(val: Self) -> Self {
        Self::unary(UnaryOp::Not, val)
    }

    // ===== Register/Memory shorthand =====

    /// Create a register read expression (alias for reg).
    pub fn read(idx: u8) -> Self {
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
        Self::binary(BinaryOp::AddW, left, right)
    }

    pub fn subw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::SubW, left, right)
    }

    pub fn sllw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::SllW, left, right)
    }

    pub fn srlw(left: Self, right: Self) -> Self {
        Self::binary(BinaryOp::SrlW, left, right)
    }

    pub fn sraw(left: Self, right: Self) -> Self {
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
        Self::unary(UnaryOp::Sext8, val)
    }

    pub fn sext16(val: Self) -> Self {
        Self::unary(UnaryOp::Sext16, val)
    }

    pub fn zext8(val: Self) -> Self {
        Self::unary(UnaryOp::Zext8, val)
    }

    pub fn zext16(val: Self) -> Self {
        Self::unary(UnaryOp::Zext16, val)
    }

    pub fn zext32(val: Self) -> Self {
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

    pub fn res_addr() -> Self {
        Self::Read(ReadExpr::ResAddr)
    }

    pub fn res_valid() -> Self {
        Self::Read(ReadExpr::ResValid)
    }

    pub fn instret() -> Self {
        Self::Read(ReadExpr::Instret)
    }

    pub fn cycle() -> Self {
        Self::Read(ReadExpr::Cycle)
    }

    pub fn temp(idx: u8) -> Self {
        Self::Read(ReadExpr::Temp(idx))
    }

    pub fn trace_idx() -> Self {
        Self::Read(ReadExpr::TraceIdx)
    }

    pub fn pc_idx() -> Self {
        Self::Read(ReadExpr::PcIdx)
    }

    pub fn exited() -> Self {
        Self::Read(ReadExpr::Exited)
    }

    pub fn exit_code() -> Self {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rv32, Rv64};

    #[test]
    fn test_expr_imm_rv64() {
        let expr = Expr::<Rv64>::imm(42);
        assert!(matches!(expr, Expr::Imm(42)));
    }

    #[test]
    fn test_expr_imm_rv32() {
        let expr = Expr::<Rv32>::imm(42);
        assert!(matches!(expr, Expr::Imm(42)));
    }

    #[test]
    fn test_expr_reg() {
        let expr = Expr::<Rv64>::reg(10);
        assert!(matches!(expr, Expr::Read(ReadExpr::Reg(10))));
    }

    #[test]
    fn test_expr_clz_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::clz(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Clz, .. }));
    }

    #[test]
    fn test_expr_ctz_rv32() {
        let val = Expr::<Rv32>::reg(5);
        let expr = Expr::ctz(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Ctz, .. }));
    }

    #[test]
    fn test_expr_cpop_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::cpop(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Cpop, .. }));
    }

    #[test]
    fn test_expr_orc8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::orc8(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Orc8, .. }));
    }

    #[test]
    fn test_expr_rev8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::rev8(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Rev8, .. }));
    }

    #[test]
    fn test_expr_clz32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::clz32(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Clz32, .. }));
    }

    #[test]
    fn test_expr_ctz32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::ctz32(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Ctz32, .. }));
    }

    #[test]
    fn test_expr_cpop32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::cpop32(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Cpop32, .. }));
    }

    #[test]
    fn test_expr_pack_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::pack(left, right);
        assert!(matches!(expr, Expr::Binary { op: BinaryOp::Pack, .. }));
    }

    #[test]
    fn test_expr_pack8_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::pack8(left, right);
        assert!(matches!(expr, Expr::Binary { op: BinaryOp::Pack8, .. }));
    }

    #[test]
    fn test_expr_pack16_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::pack16(left, right);
        assert!(matches!(expr, Expr::Binary { op: BinaryOp::Pack16, .. }));
    }

    #[test]
    fn test_expr_brev8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::brev8(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Brev8, .. }));
    }

    #[test]
    fn test_expr_zip_rv32() {
        let val = Expr::<Rv32>::reg(5);
        let expr = Expr::zip(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Zip, .. }));
    }

    #[test]
    fn test_expr_unzip_rv32() {
        let val = Expr::<Rv32>::reg(5);
        let expr = Expr::unzip(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Unzip, .. }));
    }

    #[test]
    fn test_expr_sext8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::sext8(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Sext8, .. }));
    }

    #[test]
    fn test_expr_sext16_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::sext16(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Sext16, .. }));
    }

    #[test]
    fn test_expr_sext32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::sext32(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Sext32, .. }));
    }

    #[test]
    fn test_expr_zext8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::zext8(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Zext8, .. }));
    }

    #[test]
    fn test_expr_zext16_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::zext16(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Zext16, .. }));
    }

    #[test]
    fn test_expr_zext32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::zext32(val);
        assert!(matches!(expr, Expr::Unary { op: UnaryOp::Zext32, .. }));
    }

    #[test]
    fn test_expr_select() {
        let cond = Expr::<Rv64>::reg(1);
        let then_val = Expr::<Rv64>::reg(2);
        let else_val = Expr::<Rv64>::reg(3);
        let expr = Expr::select(cond, then_val, else_val);
        assert!(matches!(expr, Expr::Ternary { op: TernaryOp::Select, .. }));
    }

    #[test]
    fn test_expr_mulh() {
        let left = Expr::<Rv64>::reg(1);
        let right = Expr::<Rv64>::reg(2);
        let expr = Expr::mulh(left, right);
        assert!(matches!(expr, Expr::Binary { op: BinaryOp::MulH, .. }));
    }

    #[test]
    fn test_expr_addw() {
        let left = Expr::<Rv64>::reg(1);
        let right = Expr::<Rv64>::reg(2);
        let expr = Expr::addw(left, right);
        assert!(matches!(expr, Expr::Binary { op: BinaryOp::AddW, .. }));
    }

    #[test]
    fn test_expr_srlw() {
        let left = Expr::<Rv64>::reg(1);
        let right = Expr::<Rv64>::reg(2);
        let expr = Expr::srlw(left, right);
        assert!(matches!(expr, Expr::Binary { op: BinaryOp::SrlW, .. }));
    }
}
