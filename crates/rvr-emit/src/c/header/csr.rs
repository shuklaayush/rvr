use super::{HeaderConfig, STATE_FIXED_REF, Xlen, reg_type};

const CSR_HEADER_PREFIX: &str = r"/* CSR access */
__attribute__((hot, pure, ";
const CSR_HEADER_MID: &str = r"always_inline))
static inline ";
const CSR_HEADER_SWITCH: &str = r" rd_csr(";
const CSR_HEADER_SWITCH_CLOSE: &str = r"uint32_t csr";
const CSR_HEADER_BODY_PREFIX: &str = r") {
    switch (csr) {
        case CSR_MCYCLE:
        case CSR_CYCLE:
        case CSR_MINSTRET:
        case CSR_INSTRET:
            return (";
const CSR_HEADER_BODY_MID: &str = r")(";
const CSR_HEADER_BODY_MID2: &str = r");
        case CSR_MCYCLEH:
        case CSR_CYCLEH:
        case CSR_MINSTRETH:
        case CSR_INSTRETH:
            return (";
const CSR_HEADER_BODY_MID3: &str = r")(";
const CSR_HEADER_BODY_SUFFIX: &str = r") >> 32);
        default:
            return ";
const CSR_HEADER_BODY_END: &str = r"->csrs[csr];
    }
}

__attribute__((hot, ";
const CSR_HEADER_WRITE_PREFIX: &str = r"always_inline))
static inline void wr_csr(";
const CSR_HEADER_WRITE_MID: &str = r"uint32_t csr, ";
const CSR_HEADER_WRITE_BODY: &str = r" val) {
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
            ";
const CSR_HEADER_WRITE_SUFFIX: &str = r"->csrs[csr] = val;
    }
}

";

const CSR_DIV_HELPERS: &str = r"/* Division helpers with RISC-V semantics */
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

";

const CSR_DIV64_HELPERS: &str = r"/* 64-bit division helpers for RV64 */
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

";

const CSR_MUL_HELPERS: &str = r"/* Multiply-high helpers */
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

";

const CSR_WORD_DIV_HELPERS: &str = r"/* RV64 word-width division helpers */
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

struct CsrHeaderArgs<'a> {
    rtype: &'a str,
    instret_param: &'a str,
    state_param_rd: &'a str,
    state_param_wr: &'a str,
    state_ref: &'a str,
    nonnull: &'a str,
    instret_val: &'a str,
}

fn push_csr_header(out: &mut String, args: &CsrHeaderArgs<'_>) {
    out.push_str(CSR_HEADER_PREFIX);
    out.push_str(args.nonnull);
    out.push_str(CSR_HEADER_MID);
    out.push_str(args.rtype);
    out.push_str(CSR_HEADER_SWITCH);
    out.push_str(args.state_param_rd);
    out.push_str(CSR_HEADER_SWITCH_CLOSE);
    out.push_str(args.instret_param);
    out.push_str(CSR_HEADER_BODY_PREFIX);
    out.push_str(args.rtype);
    out.push_str(CSR_HEADER_BODY_MID);
    out.push_str(args.instret_val);
    out.push_str(CSR_HEADER_BODY_MID2);
    out.push_str(args.rtype);
    out.push_str(CSR_HEADER_BODY_MID3);
    out.push_str(args.instret_val);
    out.push_str(CSR_HEADER_BODY_SUFFIX);
    out.push_str(args.state_ref);
    out.push_str(CSR_HEADER_BODY_END);
    out.push_str(args.nonnull);
    out.push_str(CSR_HEADER_WRITE_PREFIX);
    out.push_str(args.state_param_wr);
    out.push_str(CSR_HEADER_WRITE_MID);
    out.push_str(args.rtype);
    out.push_str(CSR_HEADER_WRITE_BODY);
    out.push_str(args.state_ref);
    out.push_str(CSR_HEADER_WRITE_SUFFIX);
}

pub(super) fn gen_csr_functions<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let rtype = reg_type::<X>();
    let instret_param = if cfg.instret_mode.counts() {
        ", uint64_t instret"
    } else {
        ""
    };

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

    let mut out = String::new();
    let args = CsrHeaderArgs {
        rtype,
        instret_param,
        state_param_rd,
        state_param_wr,
        state_ref,
        nonnull,
        instret_val: &instret_val,
    };
    push_csr_header(&mut out, &args);
    out.push_str(CSR_DIV_HELPERS);
    out.push_str(CSR_DIV64_HELPERS);
    out.push_str(CSR_MUL_HELPERS);
    out.push_str(CSR_WORD_DIV_HELPERS);
    out
}
