pub(super) fn gen_helpers() -> String {
    r"/* Zbb/Zbkb helpers: loop-free, constant-time */

/* ORC.B: set each byte to 0xFF if non-zero, else 0x00 */
static inline uint32_t rv_orc_b32(uint32_t x) {
    x |= x >> 4; x |= x >> 2; x |= x >> 1;
    x &= 0x01010101u;
    x |= x << 1; x |= x << 2; x |= x << 4;
    return x;
}

static inline uint64_t rv_orc_b64(uint64_t x) {
    x |= x >> 4; x |= x >> 2; x |= x >> 1;
    x &= 0x0101010101010101ull;
    x |= x << 1; x |= x << 2; x |= x << 4;
    return x;
}

/* BREV8: reverse bits within each byte */
static inline uint32_t rv_brev8_32(uint32_t x) {
    x = ((x >> 1) & 0x55555555u) | ((x & 0x55555555u) << 1);
    x = ((x >> 2) & 0x33333333u) | ((x & 0x33333333u) << 2);
    x = ((x >> 4) & 0x0F0F0F0Fu) | ((x & 0x0F0F0F0Fu) << 4);
    return x;
}

static inline uint64_t rv_brev8_64(uint64_t x) {
    x = ((x >> 1) & 0x5555555555555555ull) | ((x & 0x5555555555555555ull) << 1);
    x = ((x >> 2) & 0x3333333333333333ull) | ((x & 0x3333333333333333ull) << 2);
    x = ((x >> 4) & 0x0F0F0F0F0F0F0F0Full) | ((x & 0x0F0F0F0F0F0F0F0Full) << 4);
    return x;
}

/* ZIP: interleave bits [15:0] into even positions, [31:16] into odd (RV32) */
static inline uint32_t rv_zip32(uint32_t x) {
    uint32_t lo = x & 0xFFFFu, hi = x >> 16;
    lo = (lo | (lo << 8)) & 0x00FF00FFu; hi = (hi | (hi << 8)) & 0x00FF00FFu;
    lo = (lo | (lo << 4)) & 0x0F0F0F0Fu; hi = (hi | (hi << 4)) & 0x0F0F0F0Fu;
    lo = (lo | (lo << 2)) & 0x33333333u; hi = (hi | (hi << 2)) & 0x33333333u;
    lo = (lo | (lo << 1)) & 0x55555555u; hi = (hi | (hi << 1)) & 0x55555555u;
    return lo | (hi << 1);
}

/* UNZIP: gather even bits to [15:0], odd bits to [31:16] (RV32) */
static inline uint32_t rv_unzip32(uint32_t x) {
    uint32_t lo = x & 0x55555555u, hi = (x >> 1) & 0x55555555u;
    lo = (lo | (lo >> 1)) & 0x33333333u; hi = (hi | (hi >> 1)) & 0x33333333u;
    lo = (lo | (lo >> 2)) & 0x0F0F0F0Fu; hi = (hi | (hi >> 2)) & 0x0F0F0F0Fu;
    lo = (lo | (lo >> 4)) & 0x00FF00FFu; hi = (hi | (hi >> 4)) & 0x00FF00FFu;
    lo = (lo | (lo >> 8)) & 0x0000FFFFu; hi = (hi | (hi >> 8)) & 0x0000FFFFu;
    return lo | (hi << 16);
}

"
    .to_string()
}
