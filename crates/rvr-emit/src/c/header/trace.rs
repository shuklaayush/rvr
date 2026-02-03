use super::*;

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

    // Conditional parts based on fixed address mode
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

    format!(
        r#"/* Traced memory read helpers - call optimized base functions. */
__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint32_t val = rd_mem_u8({mem_arg}base, off);
    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline int32_t trd_mem_i8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    int32_t val = rd_mem_i8({mem_arg}base, off);
    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint32_t val = rd_mem_u16({mem_arg}base, off);
    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline int32_t trd_mem_i16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    int32_t val = rd_mem_i16({mem_arg}base, off);
    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint32_t val = rd_mem_u32({mem_arg}base, off);
    trace_mem_read_word(t, pc, op, phys_addr(base) + off, val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline int64_t trd_mem_i32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    int64_t val = rd_mem_i32({mem_arg}base, off);
    trace_mem_read_word(t, pc, op, phys_addr(base) + off, (uint32_t)val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline uint64_t trd_mem_u64(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off) {{
    uint64_t val = rd_mem_u64({mem_arg}base, off);
    trace_mem_read_dword(t, pc, op, phys_addr(base) + off, val);
    return val;
}}

/* Traced memory write helpers - call optimized base functions. */
__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u8(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    trace_mem_write_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    wr_mem_u8({mem_arg}base, off, val);
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u16(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    trace_mem_write_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    wr_mem_u16({mem_arg}base, off, val);
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u32(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    trace_mem_write_word(t, pc, op, phys_addr(base) + off, val);
    wr_mem_u32({mem_arg}base, off, val);
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u64(Tracer* t, {addr_type} pc, uint16_t op, {mem_param}{addr_type} base, int16_t off, uint64_t val) {{
    trace_mem_write_dword(t, pc, op, phys_addr(base) + off, val);
    wr_mem_u64({mem_arg}base, off, val);
}}

/* Traced register helpers - call trace functions */
__attribute__((hot, nonnull, always_inline))
static inline {rtype} trd_reg(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint8_t reg) {{
    {rtype} val = {state_ref}->regs[reg];
    trace_reg_read(t, pc, op, reg, val);
    return val;
}}

__attribute__((hot, nonnull, always_inline))
static inline void twr_reg(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint8_t reg, {rtype} val) {{
    trace_reg_write(t, pc, op, reg, val);
    {state_ref}->regs[reg] = val;
}}

/* Traced hot register helpers - for registers in local vars/args */
__attribute__((hot, always_inline))
static inline {rtype} trd_regval(Tracer* t, {addr_type} pc, uint16_t op, uint8_t reg, {rtype} val) {{
    trace_reg_read(t, pc, op, reg, val);
    return val;
}}

__attribute__((hot, always_inline))
static inline {rtype} twr_regval(Tracer* t, {addr_type} pc, uint16_t op, uint8_t reg, {rtype} val) {{
    trace_reg_write(t, pc, op, reg, val);
    return val;
}}

/* Traced CSR access - call trace functions */
__attribute__((hot, nonnull))
static inline {rtype} trd_csr(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint16_t csr{instret_param}) {{
    {rtype} val = rd_csr({state_arg}csr{instret_arg});
    trace_csr_read(t, pc, op, csr, val);
    return val;
}}

__attribute__((hot, nonnull))
static inline void twr_csr(Tracer* t, {addr_type} pc, uint16_t op, {state_param}uint16_t csr, {rtype} val) {{
    trace_csr_write(t, pc, op, csr, val);
    wr_csr({state_arg}csr, val);
}}

"#,
        addr_type = addr_type,
        rtype = rtype,
        mem_param = mem_param,
        mem_arg = mem_arg,
        state_param = state_param,
        state_arg = state_arg,
        state_ref = state_ref,
        instret_param = instret_param,
        instret_arg = instret_arg,
    )
}
