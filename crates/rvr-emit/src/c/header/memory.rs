use super::*;

pub(super) fn gen_memory_functions<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let addr_type = reg_type::<X>();

    // Address translation mode:
    // - Unchecked: assume valid + passthrough, guard pages catch OOB
    // - Wrap: mask to memory size, matches sv39/sv48 behavior
    // - Bounds: trap on invalid + assume + mask, explicit errors
    // Generate phys_addr body based on AddressMode semantics
    let mode = cfg.address_mode;
    let phys_addr_body = if mode.assumes_valid() {
        // Unchecked: assume valid, no masking (guard pages catch OOB)
        "    __builtin_assume(addr <= RV_MEMORY_MASK);\n    return addr;".to_string()
    } else if mode.needs_bounds_check() {
        // Bounds: check bounds + trap + mask
        format!(
            "    if (unlikely((int{0}_t)(addr << ({0} - MEMORY_BITS)) >> ({0} - MEMORY_BITS) != (int{0}_t)addr)) __builtin_trap();\n    __builtin_assume(addr <= RV_MEMORY_MASK);\n    return addr & RV_MEMORY_MASK;",
            X::VALUE
        )
    } else {
        // Wrap: mask only
        "    return addr & RV_MEMORY_MASK;".to_string()
    };

    // Conditional parts based on fixed address mode
    let (mem_param, mem_ref, nonnull) = if cfg.fixed_addresses.is_some() {
        ("", MEMORY_FIXED_REF, "")
    } else {
        ("uint8_t* restrict memory, ", "memory", "nonnull, ")
    };

    format!(
        r#"/* Translate virtual address to physical. */
static inline {addr_type} phys_addr({addr_type} addr) {{
{phys_addr_body}
}}

/* Memory access: compute phys base first, then add offset. */
__attribute__((hot, pure, {nonnull}always_inline))
static inline uint32_t rd_mem_u8({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    return phys[off];
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline int32_t rd_mem_i8({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    return (int8_t)phys[off];
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline uint32_t rd_mem_u16({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 2);
    uint16_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline int32_t rd_mem_i16({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 2);
    uint16_t val;
    memcpy(&val, ptr, sizeof(val));
    return (int16_t)val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline uint32_t rd_mem_u32({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 4);
    uint32_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline int64_t rd_mem_i32({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 4);
    uint32_t val;
    memcpy(&val, ptr, sizeof(val));
    return (int32_t)val;
}}

__attribute__((hot, pure, {nonnull}always_inline))
static inline uint64_t rd_mem_u64({mem_param}{addr_type} base, int16_t off) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    const void* ptr = __builtin_assume_aligned(phys + off, 8);
    uint64_t val;
    memcpy(&val, ptr, sizeof(val));
    return val;
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u8({mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    phys[off] = (uint8_t)val;
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u16({mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 2);
    memcpy(ptr, &val, 2);
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u32({mem_param}{addr_type} base, int16_t off, uint32_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 4);
    memcpy(ptr, &val, sizeof(val));
}}

__attribute__((hot, {nonnull}always_inline))
static inline void wr_mem_u64({mem_param}{addr_type} base, int16_t off, uint64_t val) {{
    uint8_t* phys = &{mem_ref}[phys_addr(base)];
    void* ptr = __builtin_assume_aligned(phys + off, 8);
    memcpy(ptr, &val, sizeof(val));
}}

"#,
        addr_type = addr_type,
        phys_addr_body = phys_addr_body,
        mem_param = mem_param,
        mem_ref = mem_ref,
        nonnull = nonnull,
    )
}
