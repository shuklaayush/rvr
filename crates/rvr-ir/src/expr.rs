//! Expression IR.

use crate::xlen::Xlen;

/// Expression node kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ExprKind {
    // Leaves
    Imm,      // Immediate value
    Read,     // Register/memory/CSR read
    PcConst,  // Static PC value
    Var,      // C variable reference

    // Arithmetic
    Add,
    Sub,
    Mul,
    MulH,     // Upper XLEN bits of signed*signed
    MulHSU,   // Upper XLEN bits of signed*unsigned
    MulHU,    // Upper XLEN bits of unsigned*unsigned
    Div,
    DivU,
    Rem,
    RemU,

    // Bitwise
    And,
    Or,
    Xor,
    Sll,
    Srl,
    Sra,
    Not,
    Neg,      // Unary negation

    // Comparison
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,

    // RV64 32-bit operations (sign-extend result to XLEN)
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

    // Sign/zero extension
    Sext8,
    Sext16,
    Sext32,
    Zext8,
    Zext16,
    Zext32,

    // Zbb bit manipulation
    Clz,      // Count leading zeros
    Ctz,      // Count trailing zeros
    Cpop,     // Population count
    Clz32,    // Count leading zeros (32-bit)
    Ctz32,    // Count trailing zeros (32-bit)
    Cpop32,   // Population count (32-bit)
    Orc8,     // OR-combine bytes
    Rev8,     // Byte-reverse register

    // Zbkb bit manipulation
    Pack,     // Pack lower halves
    Pack8,    // Pack lowest bytes
    Pack16,   // Pack lower 16-bits, sign-extend (RV64)
    Brev8,    // Bit-reverse each byte
    Zip,      // Bit interleave (RV32 only)
    Unzip,    // Bit deinterleave (RV32 only)

    // Ternary
    Select,

    // External call
    ExternCall,
}

/// Address spaces for reads/writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Space {
    Reg,
    Mem,
    Csr,
    Pc,
    Cycle,
    Instret,
    Temp,
    // Tracing
    TraceIdx,
    PcIdx,
    // LR/SC reservation
    ResAddr,
    ResValid,
    // Exit state
    Exited,
    ExitCode,
}

/// Expression tree node.
#[derive(Clone, Debug)]
pub struct Expr<X: Xlen> {
    pub kind: ExprKind,
    pub imm: X::Reg,
    pub space: Space,
    pub width: u8,
    pub signed: bool,
    pub mem_offset: i16,
    pub left: Option<Box<Expr<X>>>,
    pub right: Option<Box<Expr<X>>>,
    pub third: Option<Box<Expr<X>>>,
    pub var_name: Option<String>,
    pub extern_fn: Option<String>,
    pub extern_args: Vec<Expr<X>>,
}

impl<X: Xlen> Default for Expr<X> {
    fn default() -> Self {
        Self {
            kind: ExprKind::Imm,
            imm: X::from_u64(0),
            space: Space::Reg,
            width: X::REG_BYTES as u8,
            signed: false,
            mem_offset: 0,
            left: None,
            right: None,
            third: None,
            var_name: None,
            extern_fn: None,
            extern_args: Vec::new(),
        }
    }
}

impl<X: Xlen> Expr<X> {
    /// Create an immediate expression.
    pub fn imm(val: X::Reg) -> Self {
        Self {
            kind: ExprKind::Imm,
            imm: val,
            ..Default::default()
        }
    }

    /// Create a register read expression.
    pub fn reg(idx: u8) -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Reg,
            imm: X::from_u64(idx as u64),
            ..Default::default()
        }
    }

    /// Create a memory read expression.
    pub fn mem(base: Self, offset: i16, width: u8, signed: bool) -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Mem,
            width,
            signed,
            mem_offset: offset,
            left: Some(Box::new(base)),
            ..Default::default()
        }
    }

    /// Create a CSR read expression.
    pub fn csr(csr: u16) -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Csr,
            imm: X::from_u64(csr as u64),
            ..Default::default()
        }
    }

    /// Create a PC constant expression.
    pub fn pc_const(pc: X::Reg) -> Self {
        Self {
            kind: ExprKind::PcConst,
            imm: pc,
            ..Default::default()
        }
    }

    /// Create a C variable reference.
    pub fn var(name: &str) -> Self {
        Self {
            kind: ExprKind::Var,
            var_name: Some(name.to_string()),
            ..Default::default()
        }
    }

    /// Create an external function call expression.
    pub fn extern_call(fn_name: &str, args: Vec<Self>, ret_width: u8) -> Self {
        Self {
            kind: ExprKind::ExternCall,
            width: ret_width,
            extern_fn: Some(fn_name.to_string()),
            extern_args: args,
            ..Default::default()
        }
    }

    /// Create a binary operation.
    fn binop(kind: ExprKind, left: Self, right: Self) -> Self {
        Self {
            kind,
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            ..Default::default()
        }
    }

    pub fn add(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Add, left, right)
    }

    pub fn sub(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Sub, left, right)
    }

    pub fn mul(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Mul, left, right)
    }

    pub fn and(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::And, left, right)
    }

    pub fn or(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Or, left, right)
    }

    pub fn xor(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Xor, left, right)
    }

    pub fn sll(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Sll, left, right)
    }

    pub fn srl(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Srl, left, right)
    }

    pub fn sra(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Sra, left, right)
    }

    pub fn eq(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Eq, left, right)
    }

    pub fn ne(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Ne, left, right)
    }

    pub fn lt(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Lt, left, right)
    }

    pub fn ge(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Ge, left, right)
    }

    pub fn ltu(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Ltu, left, right)
    }

    pub fn geu(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Geu, left, right)
    }

    /// Create a ternary select (cond ? then : else).
    pub fn select(cond: Self, then_val: Self, else_val: Self) -> Self {
        Self {
            kind: ExprKind::Select,
            left: Some(Box::new(cond)),
            right: Some(Box::new(then_val)),
            third: Some(Box::new(else_val)),
            ..Default::default()
        }
    }

    /// Sign extend from 32 bits.
    pub fn sext32(val: Self) -> Self {
        Self {
            kind: ExprKind::Sext32,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    /// Create a NOT expression.
    pub fn not(val: Self) -> Self {
        Self {
            kind: ExprKind::Not,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    // ===== Register/Memory shorthand =====

    /// Create a register read expression (alias for reg).
    pub fn read(idx: u8) -> Self {
        Self::reg(idx)
    }

    /// Create a memory read with computed address (unsigned).
    pub fn mem_u(addr: Self, width: u8) -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Mem,
            width,
            signed: false,
            left: Some(Box::new(addr)),
            ..Default::default()
        }
    }

    /// Create a signed memory read with computed address.
    pub fn mem_s(addr: Self, width: u8) -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Mem,
            width,
            signed: true,
            left: Some(Box::new(addr)),
            ..Default::default()
        }
    }

    // ===== Division/Remainder =====

    pub fn div(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Div, left, right)
    }

    pub fn divu(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::DivU, left, right)
    }

    pub fn rem(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Rem, left, right)
    }

    pub fn remu(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::RemU, left, right)
    }

    // ===== RV64 Word operations =====

    pub fn addw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::AddW, left, right)
    }

    pub fn subw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::SubW, left, right)
    }

    pub fn sllw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::SllW, left, right)
    }

    pub fn srlw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::SrlW, left, right)
    }

    pub fn sraw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::SraW, left, right)
    }

    pub fn mulw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::MulW, left, right)
    }

    pub fn divw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::DivW, left, right)
    }

    pub fn divuw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::DivUW, left, right)
    }

    pub fn remw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::RemW, left, right)
    }

    pub fn remuw(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::RemUW, left, right)
    }

    // ===== M extension high bits =====

    pub fn mulh(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::MulH, left, right)
    }

    pub fn mulhsu(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::MulHSU, left, right)
    }

    pub fn mulhu(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::MulHU, left, right)
    }

    // ===== Unary negation =====

    pub fn neg(val: Self) -> Self {
        Self {
            kind: ExprKind::Neg,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    // ===== Zbb bit manipulation =====

    pub fn clz(val: Self) -> Self {
        Self {
            kind: ExprKind::Clz,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn ctz(val: Self) -> Self {
        Self {
            kind: ExprKind::Ctz,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn cpop(val: Self) -> Self {
        Self {
            kind: ExprKind::Cpop,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn clz32(val: Self) -> Self {
        Self {
            kind: ExprKind::Clz32,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn ctz32(val: Self) -> Self {
        Self {
            kind: ExprKind::Ctz32,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn cpop32(val: Self) -> Self {
        Self {
            kind: ExprKind::Cpop32,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn orc8(val: Self) -> Self {
        Self {
            kind: ExprKind::Orc8,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn rev8(val: Self) -> Self {
        Self {
            kind: ExprKind::Rev8,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn sext8(val: Self) -> Self {
        Self {
            kind: ExprKind::Sext8,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn sext16(val: Self) -> Self {
        Self {
            kind: ExprKind::Sext16,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn zext8(val: Self) -> Self {
        Self {
            kind: ExprKind::Zext8,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn zext16(val: Self) -> Self {
        Self {
            kind: ExprKind::Zext16,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn zext32(val: Self) -> Self {
        Self {
            kind: ExprKind::Zext32,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    // ===== Zbkb bit manipulation =====

    pub fn pack(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Pack, left, right)
    }

    pub fn pack8(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Pack8, left, right)
    }

    pub fn pack16(left: Self, right: Self) -> Self {
        Self::binop(ExprKind::Pack16, left, right)
    }

    pub fn brev8(val: Self) -> Self {
        Self {
            kind: ExprKind::Brev8,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn zip(val: Self) -> Self {
        Self {
            kind: ExprKind::Zip,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    pub fn unzip(val: Self) -> Self {
        Self {
            kind: ExprKind::Unzip,
            left: Some(Box::new(val)),
            ..Default::default()
        }
    }

    // ===== Space-specific reads =====

    pub fn res_addr() -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::ResAddr,
            ..Default::default()
        }
    }

    pub fn res_valid() -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::ResValid,
            ..Default::default()
        }
    }

    pub fn instret() -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Instret,
            ..Default::default()
        }
    }

    pub fn cycle() -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Cycle,
            ..Default::default()
        }
    }

    pub fn temp(idx: u8) -> Self {
        Self {
            kind: ExprKind::Read,
            space: Space::Temp,
            imm: X::from_u64(idx as u64),
            ..Default::default()
        }
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
        // Signed minimum
        let cond = Self::lt(left.clone(), right.clone());
        Self::select(cond, left, right)
    }

    pub fn max(left: Self, right: Self) -> Self {
        // Signed maximum
        let cond = Self::lt(left.clone(), right.clone());
        Self::select(cond, right, left)
    }

    pub fn minu(left: Self, right: Self) -> Self {
        // Unsigned minimum
        let cond = Self::ltu(left.clone(), right.clone());
        Self::select(cond, left, right)
    }

    pub fn maxu(left: Self, right: Self) -> Self {
        // Unsigned maximum
        let cond = Self::ltu(left.clone(), right.clone());
        Self::select(cond, right, left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rv32, Rv64};

    // ===== Basic expression construction tests =====

    #[test]
    fn test_expr_imm_rv64() {
        let expr = Expr::<Rv64>::imm(42);
        assert_eq!(expr.kind, ExprKind::Imm);
        assert_eq!(expr.imm, 42);
    }

    #[test]
    fn test_expr_imm_rv32() {
        let expr = Expr::<Rv32>::imm(42);
        assert_eq!(expr.kind, ExprKind::Imm);
        assert_eq!(expr.imm, 42);
    }

    #[test]
    fn test_expr_reg() {
        let expr = Expr::<Rv64>::reg(10);
        assert_eq!(expr.kind, ExprKind::Read);
        assert_eq!(expr.space, Space::Reg);
        assert_eq!(expr.imm, 10);
    }

    // ===== Bitmanip expression tests =====

    #[test]
    fn test_expr_clz_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::clz(val);
        assert_eq!(expr.kind, ExprKind::Clz);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_ctz_rv32() {
        let val = Expr::<Rv32>::reg(5);
        let expr = Expr::ctz(val);
        assert_eq!(expr.kind, ExprKind::Ctz);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_cpop_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::cpop(val);
        assert_eq!(expr.kind, ExprKind::Cpop);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_orc8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::orc8(val);
        assert_eq!(expr.kind, ExprKind::Orc8);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_rev8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::rev8(val);
        assert_eq!(expr.kind, ExprKind::Rev8);
        assert!(expr.left.is_some());
    }

    // ===== 32-bit bitmanip (RV64 word operations) =====

    #[test]
    fn test_expr_clz32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::clz32(val);
        assert_eq!(expr.kind, ExprKind::Clz32);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_ctz32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::ctz32(val);
        assert_eq!(expr.kind, ExprKind::Ctz32);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_cpop32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::cpop32(val);
        assert_eq!(expr.kind, ExprKind::Cpop32);
        assert!(expr.left.is_some());
    }

    // ===== Zbkb tests =====

    #[test]
    fn test_expr_pack_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::pack(left, right);
        assert_eq!(expr.kind, ExprKind::Pack);
        assert!(expr.left.is_some());
        assert!(expr.right.is_some());
    }

    #[test]
    fn test_expr_pack_rv32() {
        let left = Expr::<Rv32>::reg(5);
        let right = Expr::<Rv32>::reg(6);
        let expr = Expr::pack(left, right);
        assert_eq!(expr.kind, ExprKind::Pack);
    }

    #[test]
    fn test_expr_pack8_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::pack8(left, right);
        assert_eq!(expr.kind, ExprKind::Pack8);
    }

    #[test]
    fn test_expr_pack16_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::pack16(left, right);
        assert_eq!(expr.kind, ExprKind::Pack16);
    }

    #[test]
    fn test_expr_brev8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::brev8(val);
        assert_eq!(expr.kind, ExprKind::Brev8);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_zip_rv32() {
        // ZIP is RV32-only
        let val = Expr::<Rv32>::reg(5);
        let expr = Expr::zip(val);
        assert_eq!(expr.kind, ExprKind::Zip);
        assert!(expr.left.is_some());
    }

    #[test]
    fn test_expr_unzip_rv32() {
        // UNZIP is RV32-only
        let val = Expr::<Rv32>::reg(5);
        let expr = Expr::unzip(val);
        assert_eq!(expr.kind, ExprKind::Unzip);
        assert!(expr.left.is_some());
    }

    // ===== Extension tests =====

    #[test]
    fn test_expr_sext8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::sext8(val);
        assert_eq!(expr.kind, ExprKind::Sext8);
    }

    #[test]
    fn test_expr_sext16_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::sext16(val);
        assert_eq!(expr.kind, ExprKind::Sext16);
    }

    #[test]
    fn test_expr_sext32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::sext32(val);
        assert_eq!(expr.kind, ExprKind::Sext32);
    }

    #[test]
    fn test_expr_zext8_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::zext8(val);
        assert_eq!(expr.kind, ExprKind::Zext8);
    }

    #[test]
    fn test_expr_zext16_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::zext16(val);
        assert_eq!(expr.kind, ExprKind::Zext16);
    }

    #[test]
    fn test_expr_zext32_rv64() {
        let val = Expr::<Rv64>::reg(5);
        let expr = Expr::zext32(val);
        assert_eq!(expr.kind, ExprKind::Zext32);
    }

    // ===== Select/ternary tests =====

    #[test]
    fn test_expr_select_rv64() {
        let cond = Expr::<Rv64>::eq(Expr::reg(1), Expr::imm(0));
        let then_val = Expr::imm(100);
        let else_val = Expr::imm(200);
        let expr = Expr::select(cond, then_val, else_val);
        assert_eq!(expr.kind, ExprKind::Select);
        assert!(expr.left.is_some());
        assert!(expr.right.is_some());
        assert!(expr.third.is_some());
    }

    // ===== Min/max tests =====

    #[test]
    fn test_expr_min_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::min(left, right);
        // min is implemented as select(lt(left, right), left, right)
        assert_eq!(expr.kind, ExprKind::Select);
    }

    #[test]
    fn test_expr_max_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::max(left, right);
        assert_eq!(expr.kind, ExprKind::Select);
    }

    #[test]
    fn test_expr_minu_rv32() {
        let left = Expr::<Rv32>::reg(5);
        let right = Expr::<Rv32>::reg(6);
        let expr = Expr::minu(left, right);
        assert_eq!(expr.kind, ExprKind::Select);
    }

    #[test]
    fn test_expr_maxu_rv32() {
        let left = Expr::<Rv32>::reg(5);
        let right = Expr::<Rv32>::reg(6);
        let expr = Expr::maxu(left, right);
        assert_eq!(expr.kind, ExprKind::Select);
    }

    // ===== M extension high-bits multiplication =====

    #[test]
    fn test_expr_mulh_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::mulh(left, right);
        assert_eq!(expr.kind, ExprKind::MulH);
    }

    #[test]
    fn test_expr_mulhsu_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::mulhsu(left, right);
        assert_eq!(expr.kind, ExprKind::MulHSU);
    }

    #[test]
    fn test_expr_mulhu_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::mulhu(left, right);
        assert_eq!(expr.kind, ExprKind::MulHU);
    }

    // ===== RV64 word operations =====

    #[test]
    fn test_expr_addw_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::addw(left, right);
        assert_eq!(expr.kind, ExprKind::AddW);
    }

    #[test]
    fn test_expr_subw_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::subw(left, right);
        assert_eq!(expr.kind, ExprKind::SubW);
    }

    #[test]
    fn test_expr_sllw_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::sllw(left, right);
        assert_eq!(expr.kind, ExprKind::SllW);
    }

    #[test]
    fn test_expr_srlw_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::srlw(left, right);
        assert_eq!(expr.kind, ExprKind::SrlW);
    }

    #[test]
    fn test_expr_sraw_rv64() {
        let left = Expr::<Rv64>::reg(5);
        let right = Expr::<Rv64>::reg(6);
        let expr = Expr::sraw(left, right);
        assert_eq!(expr.kind, ExprKind::SraW);
    }
}
