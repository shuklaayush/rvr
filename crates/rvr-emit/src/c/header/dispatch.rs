use super::{HeaderConfig, Write, Xlen, reg_type};

pub(super) fn gen_fn_type<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    format!(
        r"/* Block function type */
typedef __attribute__((preserve_none)) void (*rv_fn)({});

",
        cfg.sig.params
    )
}

pub(super) fn gen_block_declarations<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let mut decls = String::from("/* Block forward declarations */\n");
    let width = if X::VALUE == 64 { 16 } else { 8 };
    for &addr in &cfg.block_addresses {
        writeln!(
            decls,
            "__attribute__((preserve_none)) void B_{addr:0width$x}({});",
            cfg.sig.params,
            addr = addr,
            width = width
        )
        .unwrap();
    }
    decls
}

/// Generate syscall runtime function declarations.
pub(super) fn gen_syscall_declarations<X: Xlen>() -> String {
    let rtype = reg_type::<X>();
    format!(
        r"/* Syscall runtime helpers (provided by runtime) */
{rtype} rv_sys_write(RvState* restrict state, {rtype} fd, {rtype} buf, {rtype} count);
{rtype} rv_sys_read(RvState* restrict state, {rtype} fd, {rtype} buf, {rtype} count);
{rtype} rv_sys_brk(RvState* restrict state, {rtype} addr);
{rtype} rv_sys_mmap(RvState* restrict state, {rtype} addr, {rtype} len, {rtype} prot, {rtype} flags, {rtype} fd, {rtype} off);
{rtype} rv_sys_fstat(RvState* restrict state, {rtype} fd, {rtype} statbuf);
{rtype} rv_sys_getrandom(RvState* restrict state, {rtype} buf, {rtype} len, {rtype} flags);
{rtype} rv_sys_clock_gettime(RvState* restrict state, {rtype} clk_id, {rtype} tp);

",
    )
}

pub(super) fn gen_dispatch<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let text_start = cfg.text_start;
    let rtype = reg_type::<X>();

    // Fast path: power-of-2 text_start allows single AND instruction
    // Slow path: subtraction needed for arbitrary text_start
    let (dispatch_body, comment) = if text_start.is_power_of_two() {
        let mask = text_start - 1;
        (
            format!("return (pc & {mask:#x}) >> 1;"),
            "/* Dispatch: (pc & mask) >> 1 */",
        )
    } else {
        tracing::debug!(
            text_start = format_args!("{:#x}", text_start),
            "text_start is not power of 2, using slower dispatch"
        );
        (
            format!("return (pc - {text_start:#x}) >> 1;"),
            "/* Dispatch: (pc - text_start) >> 1 (slower, non-power-of-2 start) */",
        )
    };

    format!(
        r"{comment}
static inline uint64_t dispatch_index({rtype} pc) {{
    {dispatch_body}
}}

extern const rv_fn dispatch_table[];

/* Runtime function - only this is needed from C */
int rv_execute_from(RvState* restrict state, {rtype} start_pc);

/* Metadata constant (read via dlsym) */
extern const uint32_t RV_TRACER_KIND;

",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::c::{gen_blocks_header, gen_header};
    use crate::{EmitConfig, EmitInputs};
    use rvr_ir::Rv64;

    #[test]
    fn test_gen_header() {
        let config = EmitConfig::<Rv64>::standard();
        let inputs = EmitInputs::new(0x8000_0000, 0x8000_0008);
        let header_cfg = HeaderConfig::new("test", &config, &inputs, vec![0x8000_0000]);
        let header = gen_header::<Rv64>(&header_cfg);

        assert!(header.contains("#pragma once"));
        assert!(header.contains("MEMORY_BITS"));
        assert!(header.contains("RvState"));
        assert!(header.contains("phys_addr"));
    }

    #[test]
    fn test_gen_blocks_header() {
        let config = EmitConfig::<Rv64>::standard();
        let inputs = EmitInputs::new(0x8000_0000, 0x8000_0008);
        let header_cfg =
            HeaderConfig::new("test", &config, &inputs, vec![0x8000_0000, 0x8000_0004]);
        let blocks = gen_blocks_header::<Rv64>(&header_cfg);

        assert!(blocks.contains("B_0000000080000000"));
        assert!(blocks.contains("B_0000000080000004"));
        assert!(blocks.contains("rv_trap"));
    }
}
