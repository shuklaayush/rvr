use super::*;

pub(super) fn gen_state_struct<X: Xlen>(cfg: &HeaderConfig<X>) -> String {
    let rtype = reg_type::<X>();
    let has_tracer = !cfg.tracer_config.is_none();

    // Use shared layout computation (single source of truth)
    let layout =
        RvStateLayout::from_params(X::REG_BYTES, cfg.num_registers, cfg.instret_mode.suspends());

    // Extract offsets from layout
    let offset_regs = layout.offset_regs;
    let offset_pc = layout.offset_pc;
    let offset_instret = layout.offset_instret;
    let offset_target_instret = layout.offset_target_instret;
    let offset_reservation_addr = layout.offset_reservation_addr;
    let offset_reservation_valid = layout.offset_reservation_valid;
    let offset_has_exited = layout.offset_has_exited;
    let offset_exit_code = layout.offset_exit_code;
    let offset_brk = layout.offset_brk;
    let offset_start_brk = layout.offset_start_brk;
    let offset_memory = layout.offset_memory;

    // Tracer if enabled (before CSRs)
    let offset_tracer = offset_memory + 8;

    // CSRs at end (huge array, rarely accessed in hot paths)
    let offset_csrs = if has_tracer {
        offset_tracer // tracer size added by C compiler
    } else {
        offset_memory + 8
    };

    // Optional suspender field
    let suspender_field = if layout.instret_suspend {
        format!(
            "    uint64_t target_instret;            /* offset {} */\n",
            offset_target_instret
        )
    } else {
        String::new()
    };

    // Compute pad offset (after exit_code)
    let offset_pad0 = offset_exit_code + 1;

    // Optional tracer field (before CSRs)
    let tracer_field = if has_tracer {
        format!(
            "\n    /* Tracer - embedded struct */\n    Tracer tracer;                      /* offset {} */\n",
            offset_tracer
        )
    } else {
        String::new()
    };

    // CSR offset comment - if tracer is present, offset depends on Tracer size
    let csr_offset_comment = if has_tracer {
        "after Tracer".to_string()
    } else {
        offset_csrs.to_string()
    };

    let mut s = format!(
        r#"/* VM State - hot fields first for cache locality */
typedef struct RvState {{
    /* Hot path fields (small offsets for efficient addressing) */
    {rtype} regs[{num_regs}];           /* offset {offset_regs} */
    {rtype} pc;                         /* offset {offset_pc} */
    uint64_t instret;                   /* offset {offset_instret} */
{suspender_field}
    /* Reservation for LR/SC */
    {rtype} reservation_addr;           /* offset {offset_reservation_addr} */
    uint8_t reservation_valid;          /* offset {offset_reservation_valid} */

    /* Execution control */
    uint8_t has_exited;                 /* offset {offset_has_exited} */
    uint8_t exit_code;                  /* offset {offset_exit_code} */
    uint8_t _pad0;                      /* offset {offset_pad0} */

    /* Heap management */
    {rtype} brk;                        /* offset {offset_brk} */
    {rtype} start_brk;                  /* offset {offset_start_brk} */

    /* Cold fields (rarely accessed in hot paths) */
    uint8_t* memory;                    /* offset {offset_memory} */
{tracer_field}
    /* CSRs at end (large array, rarely used) */
    {rtype} csrs[{num_csrs}];           /* offset {csr_offset_comment} */
}} RvState;

"#,
        rtype = rtype,
        num_regs = cfg.num_registers,
        num_csrs = NUM_CSRS,
        offset_regs = offset_regs,
        offset_pc = offset_pc,
        offset_instret = offset_instret,
        suspender_field = suspender_field,
        offset_reservation_addr = offset_reservation_addr,
        offset_reservation_valid = offset_reservation_valid,
        offset_has_exited = offset_has_exited,
        offset_exit_code = offset_exit_code,
        offset_pad0 = offset_pad0,
        offset_brk = offset_brk,
        offset_start_brk = offset_start_brk,
        offset_memory = offset_memory,
        tracer_field = tracer_field,
        csr_offset_comment = csr_offset_comment,
    );

    // Layout verification (C23 static_assert without message)
    // Only verify offsets that are statically known (not tracer-dependent)
    let mut asserts = format!(
        r#"/* Layout verification (C23 static_assert) */
static_assert(offsetof(RvState, regs) == {offset_regs});
static_assert(offsetof(RvState, pc) == {offset_pc});
static_assert(offsetof(RvState, instret) == {offset_instret});
static_assert(offsetof(RvState, reservation_addr) == {offset_reservation_addr});
static_assert(offsetof(RvState, has_exited) == {offset_has_exited});
static_assert(offsetof(RvState, brk) == {offset_brk});
static_assert(offsetof(RvState, memory) == {offset_memory});

"#,
        offset_regs = offset_regs,
        offset_pc = offset_pc,
        offset_instret = offset_instret,
        offset_reservation_addr = offset_reservation_addr,
        offset_has_exited = offset_has_exited,
        offset_brk = offset_brk,
        offset_memory = offset_memory,
    );

    // Add CSR offset verification only if no tracer (otherwise it's dynamic)
    if !has_tracer {
        writeln!(
            asserts,
            "static_assert(offsetof(RvState, csrs) == {});",
            offset_csrs
        )
        .unwrap();
        asserts.push('\n');
    }

    s.push_str(&asserts);
    s
}
