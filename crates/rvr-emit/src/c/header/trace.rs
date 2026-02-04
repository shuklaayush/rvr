use super::{HeaderConfig, STATE_FIXED_REF, Xlen, expand_template, reg_type};

const TRACE_TEMPLATE: &str = r"/* Traced memory read helpers - call optimized base functions. */
__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u8(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off) {
    uint32_t val = rd_mem_u8(@MEM_ARG@base, off);
    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    return val;
}

__attribute__((hot, nonnull, always_inline))
static inline int32_t trd_mem_i8(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off) {
    int32_t val = rd_mem_i8(@MEM_ARG@base, off);
    trace_mem_read_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    return val;
}

__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u16(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off) {
    uint32_t val = rd_mem_u16(@MEM_ARG@base, off);
    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    return val;
}

__attribute__((hot, nonnull, always_inline))
static inline int32_t trd_mem_i16(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off) {
    int32_t val = rd_mem_i16(@MEM_ARG@base, off);
    trace_mem_read_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    return val;
}

__attribute__((hot, nonnull, always_inline))
static inline uint32_t trd_mem_u32(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off) {
    uint32_t val = rd_mem_u32(@MEM_ARG@base, off);
    trace_mem_read_word(t, pc, op, phys_addr(base) + off, val);
    return val;
}

__attribute__((hot, nonnull, always_inline))
static inline int64_t trd_mem_i32(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off) {
    int64_t val = rd_mem_i32(@MEM_ARG@base, off);
    trace_mem_read_word(t, pc, op, phys_addr(base) + off, (uint32_t)val);
    return val;
}

__attribute__((hot, nonnull, always_inline))
static inline uint64_t trd_mem_u64(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off) {
    uint64_t val = rd_mem_u64(@MEM_ARG@base, off);
    trace_mem_read_dword(t, pc, op, phys_addr(base) + off, val);
    return val;
}

/* Traced memory write helpers - call optimized base functions. */
__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u8(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off, uint32_t val) {
    trace_mem_write_byte(t, pc, op, phys_addr(base) + off, (uint8_t)val);
    wr_mem_u8(@MEM_ARG@base, off, val);
}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u16(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off, uint32_t val) {
    trace_mem_write_halfword(t, pc, op, phys_addr(base) + off, (uint16_t)val);
    wr_mem_u16(@MEM_ARG@base, off, val);
}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u32(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off, uint32_t val) {
    trace_mem_write_word(t, pc, op, phys_addr(base) + off, val);
    wr_mem_u32(@MEM_ARG@base, off, val);
}

__attribute__((hot, nonnull, always_inline))
static inline void twr_mem_u64(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @MEM_PARAM@@ADDR_TYPE@ base, int16_t off, uint64_t val) {
    trace_mem_write_dword(t, pc, op, phys_addr(base) + off, val);
    wr_mem_u64(@MEM_ARG@base, off, val);
}

/* Traced register helpers - call trace functions */
__attribute__((hot, nonnull, always_inline))
static inline @RTYPE@ trd_reg(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @STATE_PARAM@uint8_t reg) {
    @RTYPE@ val = @STATE_REF@->regs[reg];
    trace_reg_read(t, pc, op, reg, val);
    return val;
}

__attribute__((hot, nonnull, always_inline))
static inline void twr_reg(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @STATE_PARAM@uint8_t reg, @RTYPE@ val) {
    trace_reg_write(t, pc, op, reg, val);
    @STATE_REF@->regs[reg] = val;
}

/* Traced hot register helpers - for registers in local vars/args */
__attribute__((hot, always_inline))
static inline @RTYPE@ trd_regval(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ val) {
    trace_reg_read(t, pc, op, reg, val);
    return val;
}

__attribute__((hot, always_inline))
static inline @RTYPE@ twr_regval(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, uint8_t reg, @RTYPE@ val) {
    trace_reg_write(t, pc, op, reg, val);
    return val;
}

/* Traced CSR access - call trace functions */
__attribute__((hot, nonnull))
static inline @RTYPE@ trd_csr(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @STATE_PARAM@uint16_t csr@INSTRET_PARAM@) {
    @RTYPE@ val = rd_csr(@STATE_ARG@csr@INSTRET_ARG@);
    trace_csr_read(t, pc, op, csr, val);
    return val;
}

__attribute__((hot, nonnull))
static inline void twr_csr(Tracer* t, @ADDR_TYPE@ pc, uint16_t op, @STATE_PARAM@uint16_t csr, @RTYPE@ val) {
    trace_csr_write(t, pc, op, csr, val);
    wr_csr(@STATE_ARG@csr, val);
}
";

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

    expand_template(
        TRACE_TEMPLATE,
        &[
            ("@RTYPE@", rtype),
            ("@ADDR_TYPE@", addr_type),
            ("@MEM_PARAM@", mem_param),
            ("@MEM_ARG@", mem_arg),
            ("@STATE_PARAM@", state_param),
            ("@STATE_ARG@", state_arg),
            ("@STATE_REF@", state_ref),
            ("@INSTRET_PARAM@", instret_param),
            ("@INSTRET_ARG@", instret_arg),
        ],
    )
}
