use super::{HeaderConfig, STATE_FIXED_REF, Xlen, expand_template, reg_type};

const CSR_TEMPLATE: &str = r"/* CSR access */
__attribute__((hot, pure, @NONNULL@always_inline))
static inline @RTYPE@ rd_csr(@STATE_PARAM_RD@uint32_t csr@INSTRET_PARAM@) {
    switch (csr) {
        case CSR_MCYCLE:
        case CSR_CYCLE:
        case CSR_MINSTRET:
        case CSR_INSTRET:
            return (@RTYPE@)(@INSTRET_VAL@);
        case CSR_MCYCLEH:
        case CSR_CYCLEH:
        case CSR_MINSTRETH:
        case CSR_INSTRETH:
            return (@RTYPE@)(@INSTRET_VAL@ >> 32);
        default:
            return @STATE_REF@->csrs[csr];
    }
}

__attribute__((hot, @NONNULL@always_inline))
static inline void wr_csr(@STATE_PARAM_WR@uint32_t csr, @RTYPE@ val) {
    switch (csr) {
        case CSR_MCYCLE:
        case CSR_MCYCLEH:
        case CSR_MINSTRET:
        case CSR_MINSTRETH:
        case CSR_CYCLE:
        case CSR_CYCLEH:
        case CSR_INSTRET:
        case CSR_INSTRETH:
            return;
        default:
            @STATE_REF@->csrs[csr] = val;
    }
}

/* Division helpers with RISC-V semantics */
static inline uint32_t rv_div(int32_t a, int32_t b) {
    if (b == 0) return RV_DIV_BY_ZERO;
    if (a == RV_INT32_MIN && b == -1) return (uint32_t)RV_INT32_MIN;
    return (uint32_t)(a / b);
}

static inline uint32_t rv_divu(uint32_t a, uint32_t b) {
    if (b == 0) return RV_DIV_BY_ZERO;
    return a / b;
}

static inline uint32_t rv_rem(int32_t a, int32_t b) {
    if (b == 0) return (uint32_t)a;
    if (a == RV_INT32_MIN && b == -1) return 0;
    return (uint32_t)(a % b);
}

static inline uint32_t rv_remu(uint32_t a, uint32_t b) {
    if (b == 0) return a;
    return a % b;
}

/* 64-bit division helpers for RV64 */
static inline uint64_t rv_div64(int64_t a, int64_t b) {
    if (b == 0) return UINT64_MAX;
    if (a == INT64_MIN && b == -1) return (uint64_t)INT64_MIN;
    return (uint64_t)(a / b);
}

static inline uint64_t rv_divu64(uint64_t a, uint64_t b) {
    if (b == 0) return UINT64_MAX;
    return a / b;
}

static inline uint64_t rv_rem64(int64_t a, int64_t b) {
    if (b == 0) return (uint64_t)a;
    if (a == INT64_MIN && b == -1) return 0;
    return (uint64_t)(a % b);
}

static inline uint64_t rv_remu64(uint64_t a, uint64_t b) {
    if (b == 0) return a;
    return a % b;
}

/* Multiply-high helpers */
static inline uint32_t rv_mulh(int32_t a, int32_t b) {
    return (uint32_t)(((int64_t)a * (int64_t)b) >> 32);
}

static inline uint32_t rv_mulhsu(int32_t a, uint32_t b) {
    return (uint32_t)(((int64_t)a * (int64_t)(uint64_t)b) >> 32);
}

static inline uint32_t rv_mulhu(uint32_t a, uint32_t b) {
    return (uint32_t)(((uint64_t)a * (uint64_t)b) >> 32);
}

static inline uint64_t rv_mulh64(int64_t a, int64_t b) {
    __int128 prod = (__int128)a * (__int128)b;
    return (uint64_t)(prod >> 64);
}

static inline uint64_t rv_mulhsu64(int64_t a, uint64_t b) {
    __int128 prod = (__int128)a * (__int128)b;
    return (uint64_t)(prod >> 64);
}

static inline uint64_t rv_mulhu64(uint64_t a, uint64_t b) {
    unsigned __int128 prod = (unsigned __int128)a * (unsigned __int128)b;
    return (uint64_t)(prod >> 64);
}

/* RV64 word-width division helpers */
static inline uint64_t rv_divw(int32_t a, int32_t b) {
    if (b == 0) return UINT64_MAX;
    if (a == INT32_MIN && b == -1) return (uint64_t)(int64_t)INT32_MIN;
    return (uint64_t)(int64_t)(a / b);
}

static inline uint64_t rv_divuw(uint32_t a, uint32_t b) {
    if (b == 0) return UINT64_MAX;
    return (uint64_t)(int64_t)(int32_t)(a / b);
}

static inline uint64_t rv_remw(int32_t a, int32_t b) {
    if (b == 0) return (uint64_t)(int64_t)a;
    if (a == INT32_MIN && b == -1) return 0;
    return (uint64_t)(int64_t)(a % b);
}

static inline uint64_t rv_remuw(uint32_t a, uint32_t b) {
    if (b == 0) return (uint64_t)(int64_t)(int32_t)a;
    return (uint64_t)(int64_t)(int32_t)(a % b);
}
";

pub(super) fn gen_csr_functions<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let rtype = reg_type::<X>();
    let instret_param = if cfg.instret_mode.counts() {
        ", uint64_t instret"
    } else {
        ""
    };

    // Conditional parts based on fixed address mode
    let (state_param_rd, state_param_wr, state_ref, nonnull, instret_val) =
        if cfg.fixed_addresses.is_some() {
            let instret = if cfg.instret_mode.counts() {
                "instret".to_string()
            } else {
                format!("{STATE_FIXED_REF}->instret")
            };
            ("", "", STATE_FIXED_REF, "", instret)
        } else {
            let instret = if cfg.instret_mode.counts() {
                "instret"
            } else {
                "s->instret"
            };
            (
                "const RvState* restrict s, ",
                "RvState* restrict s, ",
                "s",
                "nonnull, ",
                instret.to_string(),
            )
        };

    expand_template(
        CSR_TEMPLATE,
        &[
            ("@RTYPE@", rtype),
            ("@INSTRET_PARAM@", instret_param),
            ("@STATE_PARAM_RD@", state_param_rd),
            ("@STATE_PARAM_WR@", state_param_wr),
            ("@STATE_REF@", state_ref),
            ("@NONNULL@", nonnull),
            ("@INSTRET_VAL@", &instret_val),
        ],
    )
}
