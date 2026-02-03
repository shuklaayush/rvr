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
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Clz,
            ..
        }
    ));
}

#[test]
fn test_expr_ctz_rv32() {
    let val = Expr::<Rv32>::reg(5);
    let expr = Expr::ctz(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Ctz,
            ..
        }
    ));
}

#[test]
fn test_expr_cpop_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::cpop(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Cpop,
            ..
        }
    ));
}

#[test]
fn test_expr_orc8_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::orc8(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Orc8,
            ..
        }
    ));
}

#[test]
fn test_expr_rev8_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::rev8(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Rev8,
            ..
        }
    ));
}

#[test]
fn test_expr_clz32_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::clz32(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Clz32,
            ..
        }
    ));
}

#[test]
fn test_expr_ctz32_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::ctz32(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Ctz32,
            ..
        }
    ));
}

#[test]
fn test_expr_cpop32_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::cpop32(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Cpop32,
            ..
        }
    ));
}

#[test]
fn test_expr_pack_rv64() {
    let left = Expr::<Rv64>::reg(5);
    let right = Expr::<Rv64>::reg(6);
    let expr = Expr::pack(left, right);
    assert!(matches!(
        expr,
        Expr::Binary {
            op: BinaryOp::Pack,
            ..
        }
    ));
}

#[test]
fn test_expr_pack8_rv64() {
    let left = Expr::<Rv64>::reg(5);
    let right = Expr::<Rv64>::reg(6);
    let expr = Expr::pack8(left, right);
    assert!(matches!(
        expr,
        Expr::Binary {
            op: BinaryOp::Pack8,
            ..
        }
    ));
}

#[test]
fn test_expr_pack16_rv64() {
    let left = Expr::<Rv64>::reg(5);
    let right = Expr::<Rv64>::reg(6);
    let expr = Expr::pack16(left, right);
    assert!(matches!(
        expr,
        Expr::Binary {
            op: BinaryOp::Pack16,
            ..
        }
    ));
}

#[test]
fn test_expr_brev8_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::brev8(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Brev8,
            ..
        }
    ));
}

#[test]
fn test_expr_zip_rv32() {
    let val = Expr::<Rv32>::reg(5);
    let expr = Expr::zip(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Zip,
            ..
        }
    ));
}

#[test]
fn test_expr_unzip_rv32() {
    let val = Expr::<Rv32>::reg(5);
    let expr = Expr::unzip(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Unzip,
            ..
        }
    ));
}

#[test]
fn test_expr_sext8_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::sext8(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Sext8,
            ..
        }
    ));
}

#[test]
fn test_expr_sext16_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::sext16(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Sext16,
            ..
        }
    ));
}

#[test]
fn test_expr_sext32_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::sext32(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Sext32,
            ..
        }
    ));
}

#[test]
fn test_expr_zext8_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::zext8(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Zext8,
            ..
        }
    ));
}

#[test]
fn test_expr_zext16_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::zext16(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Zext16,
            ..
        }
    ));
}

#[test]
fn test_expr_zext32_rv64() {
    let val = Expr::<Rv64>::reg(5);
    let expr = Expr::zext32(val);
    assert!(matches!(
        expr,
        Expr::Unary {
            op: UnaryOp::Zext32,
            ..
        }
    ));
}

#[test]
fn test_expr_select() {
    let cond = Expr::<Rv64>::reg(1);
    let then_val = Expr::<Rv64>::reg(2);
    let else_val = Expr::<Rv64>::reg(3);
    let expr = Expr::select(cond, then_val, else_val);
    assert!(matches!(
        expr,
        Expr::Ternary {
            op: TernaryOp::Select,
            ..
        }
    ));
}

#[test]
fn test_expr_mulh() {
    let left = Expr::<Rv64>::reg(1);
    let right = Expr::<Rv64>::reg(2);
    let expr = Expr::mulh(left, right);
    assert!(matches!(
        expr,
        Expr::Binary {
            op: BinaryOp::MulH,
            ..
        }
    ));
}

#[test]
fn test_expr_addw() {
    let left = Expr::<Rv64>::reg(1);
    let right = Expr::<Rv64>::reg(2);
    let expr = Expr::addw(left, right);
    assert!(matches!(
        expr,
        Expr::Binary {
            op: BinaryOp::AddW,
            ..
        }
    ));
}

#[test]
fn test_expr_srlw() {
    let left = Expr::<Rv64>::reg(1);
    let right = Expr::<Rv64>::reg(2);
    let expr = Expr::srlw(left, right);
    assert!(matches!(
        expr,
        Expr::Binary {
            op: BinaryOp::SrlW,
            ..
        }
    ));
}
