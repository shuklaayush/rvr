//! Expression IR.

use rvr_isa::Xlen;

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

    // Comparison
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,

    // RV64 32-bit operations
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
}
