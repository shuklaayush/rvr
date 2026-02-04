use super::*;
use rvr_ir::Expr;
use rvr_isa::Rv64;

#[test]
fn test_render_imm() {
    let config = EmitConfig::<Rv64>::default();
    let emitter = CEmitter::new(config, EmitInputs::default());

    let expr = Expr::imm(42);
    let result = emitter.render_expr(&expr);
    assert_eq!(result, "0x2aULL");
}

#[test]
fn test_render_reg_read() {
    let mut config = EmitConfig::<Rv64>::default();
    config.hot_regs.clear(); // Test non-hot path
    let emitter = CEmitter::new(config, EmitInputs::default());

    // Non-hot reg uses state->regs[]
    let expr = Expr::reg(5);
    let result = emitter.render_expr(&expr);
    assert_eq!(result, "state->regs[5]");
}

#[test]
fn test_render_reg_read_hot() {
    let config = EmitConfig::<Rv64>::default();
    let emitter = CEmitter::new(config, EmitInputs::default());

    // Default config has hot regs, t0 (reg 5) should be hot
    let expr = Expr::reg(5);
    let result = emitter.render_expr(&expr);
    assert_eq!(result, "t0");
}

#[test]
fn test_render_add() {
    let mut config = EmitConfig::<Rv64>::default();
    config.hot_regs.clear(); // Test non-hot path
    let emitter = CEmitter::new(config, EmitInputs::default());

    let expr = Expr::add(Expr::reg(1), Expr::imm(10));
    let result = emitter.render_expr(&expr);
    assert_eq!(result, "(state->regs[1] + 0xaULL)");
}

#[test]
fn test_render_add_hot() {
    let config = EmitConfig::<Rv64>::default();
    let emitter = CEmitter::new(config, EmitInputs::default());

    // Default config has hot regs, ra (reg 1) should be hot
    let expr = Expr::add(Expr::reg(1), Expr::imm(10));
    let result = emitter.render_expr(&expr);
    assert_eq!(result, "(ra + 0xaULL)");
}
