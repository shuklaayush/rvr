// SPDX-License-Identifier: Apache-2.0
// Model-specific macros for rvr target in riscv-arch-test

#ifndef _MODEL_TEST_H
#define _MODEL_TEST_H

// XLEN must be defined before including this header (via -DXLEN=64 or -DXLEN=32)
#ifndef XLEN
  #if __riscv_xlen == 64
    #define XLEN 64
  #elif __riscv_xlen == 32
    #define XLEN 32
  #else
    #error "XLEN not defined and cannot be auto-detected"
  #endif
#endif

// Floating point register width (not used, but required by arch_test.h)
#ifndef FLEN
  #define FLEN 0
#endif

// Signature region bounds (must match linker script)
#define SIG_START_ADDR 0x80002000
#define SIG_END_ADDR   0x80010000

// HTIF tohost address for halt mechanism
#define TOHOST_ADDR 0x80001000

//-----------------------------------------------------------------------
// RVMODEL_BOOT - Platform boot code
//-----------------------------------------------------------------------
// Sets up entry point label that tests expect
#define RVMODEL_BOOT                                                     \
    .section .text.init;                                                 \
    .globl rvtest_entry_point;                                           \
    .globl _start;                                                       \
_start:                                                                  \
rvtest_entry_point:

//-----------------------------------------------------------------------
// RVMODEL_HALT - Test termination via HTIF
//-----------------------------------------------------------------------
#define RVMODEL_HALT                                                     \
    la t0, tohost;                                                       \
    li t1, 1;                                                            \
    sw t1, 0(t0);                                                        \
    sw zero, 4(t0);                                                      \
1:  j 1b;

//-----------------------------------------------------------------------
// RVMODEL_DATA_BEGIN/END - Signature region markers
//-----------------------------------------------------------------------
#define RVMODEL_DATA_BEGIN                                               \
    .align 4;                                                            \
    .global begin_signature;                                             \
begin_signature:

#define RVMODEL_DATA_END                                                 \
    .align 4;                                                            \
    .global end_signature;                                               \
end_signature:                                                           \
    RVMODEL_DATA_SECTION

// Place tohost/fromhost in their own section
#define RVMODEL_DATA_SECTION                                             \
    .pushsection .tohost, "aw", @progbits;                               \
    .align 8;                                                            \
    .global tohost;                                                      \
tohost:                                                                  \
    .dword 0;                                                            \
    .align 8;                                                            \
    .global fromhost;                                                    \
fromhost:                                                                \
    .dword 0;                                                            \
    .popsection;

//-----------------------------------------------------------------------
// RVMODEL_IO_* - Debug output macros (no-op for rvr)
//-----------------------------------------------------------------------
#define RVMODEL_IO_INIT
#define RVMODEL_IO_WRITE_STR(_R, _STR)
#define RVMODEL_IO_CHECK()
#define RVMODEL_IO_ASSERT_GPR_EQ(_S, _R, _I)
#define RVMODEL_IO_ASSERT_SFPR_EQ(_F, _R, _I)
#define RVMODEL_IO_ASSERT_DFPR_EQ(_D, _R, _I)

//-----------------------------------------------------------------------
// RVMODEL_*_INT - Interrupt control macros (no-op for rvr)
//-----------------------------------------------------------------------
#define RVMODEL_SET_MSW_INT
#define RVMODEL_CLEAR_MSW_INT
#define RVMODEL_SET_MTIMER_INT
#define RVMODEL_CLEAR_MTIMER_INT
#define RVMODEL_CLEAR_MEXT_INT

#endif // _MODEL_TEST_H
