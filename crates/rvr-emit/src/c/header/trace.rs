use std::fmt::Write;

use super::{HeaderConfig, STATE_FIXED_REF, Xlen, reg_type};

const TRACE_MEM_READS_HEADER: &str =
    "/* Traced memory read helpers - call optimized base functions. */\n";
const TRACE_MEM_WRITES_HEADER: &str =
    "\n/* Traced memory write helpers - call optimized base functions. */\n";
const TRACE_REG_HEADER: &str = "\n/* Traced register helpers - call trace functions */\n";
const TRACE_REGVAL_HEADER: &str =
    "\n/* Traced hot register helpers - for registers in local vars/args */\n";
const TRACE_CSR_HEADER: &str = "\n/* Traced CSR access - call trace functions */\n";

pub(super) fn gen_trace_helpers<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let rtype = reg_type::<X>();
    let addr_type = reg_type::<X>();
    let instret_param = if cfg.instret_mode.counts() {
        ", uint64_t instret"
    } else {
        ""
    };
    let instret_arg = if cfg.instret_mode.counts() {
        ", instret"
    } else {
        ""
    };

    let (mem_param, mem_arg, state_param, state_arg, state_ref) = if cfg.fixed_addresses.is_some() {
        ("", "", "", "", STATE_FIXED_REF)
    } else {
        (
            "uint8_t* restrict memory, ",
            "memory, ",
            "RvState* restrict s, ",
            "s, ",
            "s",
        )
    };

    let mut out = String::new();
    push_trace_mem_reads(&mut out, addr_type, mem_param, mem_arg);
    push_trace_mem_writes(&mut out, addr_type, mem_param, mem_arg);
    push_trace_reg_helpers(&mut out, rtype, addr_type, state_param, state_ref);
    push_trace_regval_helpers(&mut out, rtype, addr_type);
    push_trace_csr_helpers(
        &mut out,
        rtype,
        addr_type,
        state_param,
        state_arg,
        instret_param,
        instret_arg,
    );
    out
}

fn push_trace_mem_reads(out: &mut String, addr_type: &str, mem_param: &str, mem_arg: &str) {
    out.push_str(TRACE_MEM_READS_HEADER);
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline uint32_t trd_mem_u8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{\n    uint32_t val = rd_mem_u8({mem_arg}base, off);\n    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);\n    return val;\n}}\n")
    .expect("formatting trd_mem_u8");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline int32_t trd_mem_i8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{\n    int32_t val = rd_mem_i8({mem_arg}base, off);\n    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);\n    return val;\n}}\n")
    .expect("formatting trd_mem_i8");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline uint32_t trd_mem_u16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{\n    uint32_t val = rd_mem_u16({mem_arg}base, off);\n    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);\n    return val;\n}}\n")
    .expect("formatting trd_mem_u16");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline int32_t trd_mem_i16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{\n    int32_t val = rd_mem_i16({mem_arg}base, off);\n    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);\n    return val;\n}}\n")
    .expect("formatting trd_mem_i16");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline uint32_t trd_mem_u32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{\n    uint32_t val = rd_mem_u32({mem_arg}base, off);\n    trace_mem_read_word(t, pc, op, phys_addr(base) + off, val);\n    return val;\n}}\n")
    .expect("formatting trd_mem_u32");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline int64_t trd_mem_i32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{\n    int64_t val = rd_mem_i32({mem_arg}base, off);\n    trace_mem_read_word(t, pc, op, phys_addr(base) + off, (uint32_t)val);\n    return val;\n}}\n")
    .expect("formatting trd_mem_i32");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline uint64_t trd_mem_u64(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{\n    uint64_t val = rd_mem_u64({mem_arg}base, off);\n    trace_mem_read_dword(t, pc, op, phys_addr(base) + off, val);\n    return val;\n}}")
    .expect("formatting trd_mem_u64");
}

fn push_trace_mem_writes(out: &mut String, addr_type: &str, mem_param: &str, mem_arg: &str) {
    out.push_str(TRACE_MEM_WRITES_HEADER);
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline void twr_mem_u8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{\n    trace_mem_write_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);\n    wr_mem_u8({mem_arg}base, off, val);\n}}\n")
    .expect("formatting twr_mem_u8");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline void twr_mem_u16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{\n    trace_mem_write_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);\n    wr_mem_u16({mem_arg}base, off, val);\n}}\n")
    .expect("formatting twr_mem_u16");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline void twr_mem_u32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{\n    trace_mem_write_word(t, pc, op, phys_addr(base) + off, val);\n    wr_mem_u32({mem_arg}base, off, val);\n}}\n")
    .expect("formatting twr_mem_u32");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline void twr_mem_u64(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint64_t val) {{\n    trace_mem_write_dword(t, pc, op, phys_addr(base) + off, val);\n    wr_mem_u64({mem_arg}base, off, val);\n}}")
    .expect("formatting twr_mem_u64");
}

fn push_trace_reg_helpers(
    out: &mut String,
    rtype: &str,
    addr_type: &str,
    state_param: &str,
    state_ref: &str,
) {
    out.push_str(TRACE_REG_HEADER);
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline {rtype} trd_reg(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint8_t reg) {{\n    {rtype} val = {state_ref}->regs[reg];\n    trace_reg_read(t, pc, op, reg, val);\n    return val;\n}}\n")
    .expect("formatting trd_reg");
    writeln!(
        out,
        "__attribute__((hot, nonnull, always_inline))\nstatic inline void twr_reg(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint8_t reg, {rtype} val) {{\n    trace_reg_write(t, pc, op, reg, val);\n    {state_ref}->regs[reg] = val;\n}}")
    .expect("formatting twr_reg");
}

fn push_trace_regval_helpers(out: &mut String, rtype: &str, addr_type: &str) {
    out.push_str(TRACE_REGVAL_HEADER);
    writeln!(
        out,
        "__attribute__((hot, always_inline))\nstatic inline {rtype} trd_regval(Tracer* t, {addr_type} pc, uint16_t op, uint8_t reg, {rtype} val) {{\n    trace_reg_read(t, pc, op, reg, val);\n    return val;\n}}\n")
    .expect("formatting trd_regval");
    writeln!(
        out,
        "__attribute__((hot, always_inline))\nstatic inline {rtype} twr_regval(Tracer* t, {addr_type} pc, uint16_t op, uint8_t reg, {rtype} val) {{\n    trace_reg_write(t, pc, op, reg, val);\n    return val;\n}}")
    .expect("formatting twr_regval");
}

fn push_trace_csr_helpers(
    out: &mut String,
    rtype: &str,
    addr_type: &str,
    state_param: &str,
    state_arg: &str,
    instret_param: &str,
    instret_arg: &str,
) {
    out.push_str(TRACE_CSR_HEADER);
    writeln!(
        out,
        "__attribute__((hot, nonnull))\nstatic inline {rtype} trd_csr(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint16_t csr{instret_param}) {{\n    {rtype} val = rd_csr({state_arg}csr{instret_arg});\n    trace_csr_read(t, pc, op, csr, val);\n    return val;\n}}\n")
    .expect("formatting trd_csr");
    writeln!(
        out,
        "__attribute__((hot, nonnull))\nstatic inline void twr_csr(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint16_t csr, {rtype} val) {{\n    trace_csr_write(t, pc, op, csr, val);\n    wr_csr({state_arg}csr, val);\n}}")
    .expect("formatting twr_csr");
}
