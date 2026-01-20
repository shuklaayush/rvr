#!/usr/bin/env python3
"""Emit a C tracer header skeleton from a Rust tracer file.

Usage:
  scripts/emit_tracer_header.py path/to/tracer.rs output.h --xlen 64
"""

import argparse
import re
from pathlib import Path

NAME_RE = re.compile(r"TRACER_NAME:\s*&str\s*=\s*\"([^\"]+)\"")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("rust_tracer", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--xlen", type=int, choices=(32, 64), default=64)
    args = parser.parse_args()

    content = args.rust_tracer.read_text()
    match = NAME_RE.search(content)
    name = match.group(1) if match else args.rust_tracer.stem

    rtype = "uint64_t" if args.xlen == 64 else "uint32_t"

    header = f"""/* Auto-generated tracer header skeleton for {name}. */
#pragma once

#include <stdint.h>

typedef struct Tracer {{
    /* add fields here */
}} Tracer;

static inline void trace_init(Tracer* t) {{ (void)t; }}
static inline void trace_fini(Tracer* t) {{ (void)t; }}

static inline void trace_block(Tracer* t, {rtype} pc) {{ (void)t; (void)pc; }}
static inline void trace_pc(Tracer* t, {rtype} pc, uint16_t op) {{ (void)t; (void)pc; (void)op; }}
static inline void trace_opcode(Tracer* t, {rtype} pc, uint16_t op, uint32_t opcode) {{ (void)t; (void)pc; (void)op; (void)opcode; }}

static inline void trace_reg_read(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}}
static inline void trace_reg_write(Tracer* t, {rtype} pc, uint16_t op, uint8_t reg, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)reg; (void)value;
}}

static inline void trace_mem_read_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_read_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_read_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_read_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}

static inline void trace_mem_write_byte(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint8_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_write_halfword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint16_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_write_word(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint32_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}
static inline void trace_mem_write_dword(Tracer* t, {rtype} pc, uint16_t op, {rtype} addr, uint64_t value) {{
    (void)t; (void)pc; (void)op; (void)addr; (void)value;
}}

static inline void trace_branch_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}
static inline void trace_branch_not_taken(Tracer* t, {rtype} pc, uint16_t op, {rtype} target) {{
    (void)t; (void)pc; (void)op; (void)target;
}}

static inline void trace_csr_read(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}
static inline void trace_csr_write(Tracer* t, {rtype} pc, uint16_t op, uint16_t csr, {rtype} value) {{
    (void)t; (void)pc; (void)op; (void)csr; (void)value;
}}
"""

    args.output.write_text(header)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
