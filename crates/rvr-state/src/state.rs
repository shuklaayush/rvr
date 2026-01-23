//! RISC-V machine state struct.
//!
//! Layout must match the generated C `RvState` struct exactly.

use rvr_ir::Xlen;

use crate::suspender::SuspenderState;
use crate::tracer::TracerState;

/// Number of CSRs.
pub const NUM_CSRS: usize = 4096;

/// Number of registers for I extension (32 GPRs).
pub const NUM_REGS_I: usize = 32;

/// Number of registers for E extension (16 GPRs).
pub const NUM_REGS_E: usize = 16;

/// RISC-V machine state.
///
/// This struct has a C-compatible layout matching the generated C header.
/// The layout is parameterized by:
/// - `X`: Register width (Rv32 or Rv64)
/// - `T`: Tracer state type (ZST when `()`, real struct when tracing)
/// - `S`: Suspender state type (ZST when `()`, real struct when suspending)
/// - `NUM_REGS`: Number of general-purpose registers (32 for I, 16 for E)
///
/// # Layout
///
/// ```text
/// offset 0:     memory (*mut u8)
/// offset 8:     regs[NUM_REGS]
/// offset ?:     csrs[4096]
/// offset ?:     pc
/// offset ?:     _pad0 (u32)
/// offset ?:     instret (u64, 8-byte aligned)
/// offset ?:     suspender (only when S != (), right after instret for C compatibility)
/// offset ?:     reservation_addr
/// offset ?:     reservation_valid (u8)
/// offset ?:     has_exited (u8)
/// offset ?:     exit_code (u8)
/// offset ?:     _pad1 (u8)
/// offset ?:     _pad2 (i64, 8-byte aligned)
/// offset ?:     brk
/// offset ?:     start_brk
/// offset ?:     tracer (only when T != ())
/// ```
#[repr(C)]
pub struct RvState<
    X: Xlen,
    T: TracerState = (),
    S: SuspenderState = (),
    const NUM_REGS: usize = NUM_REGS_I,
> {
    /// Guest memory pointer.
    pub memory: *mut u8,

    /// General-purpose registers.
    pub regs: [X::Reg; NUM_REGS],

    /// Control and status registers.
    pub csrs: [X::Reg; NUM_CSRS],

    /// Program counter.
    pub pc: X::Reg,

    /// Alignment padding.
    _pad0: u32,

    /// Instructions retired counter.
    pub instret: u64,

    /// Suspender state (ZST when S = (), real struct when suspending).
    /// Placed right after instret for C layout compatibility.
    pub suspender: S,

    /// Reservation address for LR/SC.
    pub reservation_addr: X::Reg,

    /// Reservation valid flag for LR/SC.
    pub reservation_valid: u8,

    /// Has the VM exited?
    pub has_exited: u8,

    /// Exit code (valid when has_exited is true).
    pub exit_code: u8,

    /// Alignment padding.
    _pad1: u8,

    /// Alignment padding (8-byte alignment for brk).
    _pad2: i64,

    /// Current heap break.
    pub brk: X::Reg,

    /// Initial heap break.
    pub start_brk: X::Reg,

    /// Tracer state (ZST when T = (), real struct when tracing).
    pub tracer: T,
}

impl<X: Xlen, T: TracerState, S: SuspenderState, const NUM_REGS: usize> RvState<X, T, S, NUM_REGS> {
    /// Create a new zeroed state.
    ///
    /// # Safety
    ///
    /// The memory pointer must be valid for the lifetime of the state,
    /// or null if memory will be set later.
    pub fn new() -> Self {
        Self {
            memory: std::ptr::null_mut(),
            regs: [X::from_u64(0); NUM_REGS],
            csrs: [X::from_u64(0); NUM_CSRS],
            pc: X::from_u64(0),
            _pad0: 0,
            instret: 0,
            suspender: S::default(),
            reservation_addr: X::from_u64(0),
            reservation_valid: 0,
            has_exited: 0,
            exit_code: 0,
            _pad1: 0,
            _pad2: 0,
            brk: X::from_u64(0),
            start_brk: X::from_u64(0),
            tracer: T::default(),
        }
    }

    /// Tracer kind ID for C API.
    pub const fn tracer_kind() -> u32 {
        T::KIND
    }

    /// Whether suspender adds fields to the struct.
    pub const fn has_suspender() -> bool {
        S::HAS_FIELDS
    }

    /// Get state as a mutable pointer (for FFI).
    pub fn as_mut_ptr(&mut self) -> *mut Self {
        self as *mut Self
    }

    /// Get state as a void pointer (for FFI).
    pub fn as_void_ptr(&mut self) -> *mut std::ffi::c_void {
        self as *mut Self as *mut std::ffi::c_void
    }

    /// Reset state to initial values.
    pub fn reset(&mut self) {
        self.regs = [X::from_u64(0); NUM_REGS];
        self.pc = X::from_u64(0);
        self.instret = 0;
        self.reservation_addr = X::from_u64(0);
        self.reservation_valid = 0;
        self.has_exited = 0;
        self.exit_code = 0;
    }

    /// Check if the VM has exited.
    pub fn has_exited(&self) -> bool {
        self.has_exited != 0
    }

    /// Get the exit code (only valid if has_exited is true).
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }

    /// Clear the exit flag to allow further execution.
    pub fn clear_exit(&mut self) {
        self.has_exited = 0;
        self.exit_code = 0;
    }

    /// Get the instruction count.
    pub fn instret(&self) -> u64 {
        self.instret
    }

    /// Get the program counter.
    pub fn pc(&self) -> X::Reg {
        self.pc
    }

    /// Set the program counter.
    pub fn set_pc(&mut self, pc: X::Reg) {
        self.pc = pc;
    }

    /// Get a register value.
    pub fn get_reg(&self, idx: usize) -> X::Reg {
        self.regs[idx]
    }

    /// Set a register value.
    pub fn set_reg(&mut self, idx: usize, val: X::Reg) {
        if idx != 0 {
            self.regs[idx] = val;
        }
    }

    /// Set memory pointer.
    pub fn set_memory(&mut self, memory: *mut u8) {
        self.memory = memory;
    }

    /// Get memory pointer.
    pub fn memory(&self) -> *mut u8 {
        self.memory
    }
}

impl<X: Xlen, T: TracerState, S: SuspenderState, const NUM_REGS: usize> Default
    for RvState<X, T, S, NUM_REGS>
{
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for RV32I state (32-bit, 32 registers, no tracer, no suspender).
pub type Rv32State = RvState<rvr_ir::Rv32, (), (), NUM_REGS_I>;

/// Type alias for RV64I state (64-bit, 32 registers, no tracer, no suspender).
pub type Rv64State = RvState<rvr_ir::Rv64, (), (), NUM_REGS_I>;

/// Type alias for RV32E state (32-bit, 16 registers, no tracer, no suspender).
pub type Rv32EState = RvState<rvr_ir::Rv32, (), (), NUM_REGS_E>;

/// Type alias for RV64E state (64-bit, 16 registers, no tracer, no suspender).
pub type Rv64EState = RvState<rvr_ir::Rv64, (), (), NUM_REGS_E>;

/// Type alias for RV32I state with tracer (no suspender).
pub type Rv32StateWith<T> = RvState<rvr_ir::Rv32, T, (), NUM_REGS_I>;

/// Type alias for RV64I state with tracer (no suspender).
pub type Rv64StateWith<T> = RvState<rvr_ir::Rv64, T, (), NUM_REGS_I>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suspender::InstretSuspender;
    use crate::tracer::PreflightTracer;
    use memoffset::offset_of;
    use rvr_ir::{Rv32, Rv64};
    use std::mem::size_of;

    #[test]
    fn test_rv64_state_layout() {
        // These offsets must match the generated C header for RV64 with 32 regs.
        // When T = () and S = (), both are ZST and don't affect layout.
        // Suspender is positioned after instret (ZST when S = ()).
        assert_eq!(offset_of!(Rv64State, memory), 0);
        assert_eq!(offset_of!(Rv64State, regs), 8);
        assert_eq!(offset_of!(Rv64State, csrs), 8 + 32 * 8); // 264
        assert_eq!(offset_of!(Rv64State, pc), 264 + 4096 * 8); // 33032
        assert_eq!(offset_of!(Rv64State, _pad0), 33032 + 8); // 33040
        assert_eq!(offset_of!(Rv64State, instret), 33048);
        // Suspender ZST is here at 33056, adds 0 bytes
        assert_eq!(offset_of!(Rv64State, reservation_addr), 33056);
        assert_eq!(offset_of!(Rv64State, reservation_valid), 33064);
        assert_eq!(offset_of!(Rv64State, has_exited), 33065);
        assert_eq!(offset_of!(Rv64State, exit_code), 33066);
        assert_eq!(offset_of!(Rv64State, _pad1), 33067);
        assert_eq!(offset_of!(Rv64State, _pad2), 33072);
        assert_eq!(offset_of!(Rv64State, brk), 33080);
        assert_eq!(offset_of!(Rv64State, start_brk), 33088);
        assert_eq!(size_of::<Rv64State>(), 33096);
    }

    #[test]
    fn test_rv32_state_layout() {
        // For RV32 with 32 regs, no tracer, no suspender
        assert_eq!(offset_of!(RvState<Rv32, (), (), 32>, memory), 0);
        assert_eq!(offset_of!(RvState<Rv32, (), (), 32>, regs), 8);
        assert_eq!(offset_of!(RvState<Rv32, (), (), 32>, csrs), 8 + 32 * 4); // 136
        assert_eq!(offset_of!(RvState<Rv32, (), (), 32>, pc), 136 + 4096 * 4); // 16520
    }

    #[test]
    fn test_rv64e_state_layout() {
        // For RV64 with 16 regs (E extension)
        assert_eq!(offset_of!(Rv64EState, memory), 0);
        assert_eq!(offset_of!(Rv64EState, regs), 8);
        assert_eq!(offset_of!(Rv64EState, csrs), 8 + 16 * 8); // 136
        assert_eq!(offset_of!(Rv64EState, pc), 136 + 4096 * 8); // 32904
    }

    #[test]
    fn test_rv64_state_with_preflight_tracer() {
        // When tracer is PreflightTracer (32 bytes), it adds at the end
        type StateWithTracer = Rv64StateWith<PreflightTracer<Rv64>>;
        let base_size = size_of::<Rv64State>();
        let tracer_size = size_of::<PreflightTracer<Rv64>>();
        assert_eq!(tracer_size, 32);
        assert_eq!(size_of::<StateWithTracer>(), base_size + tracer_size);

        // Tracer offset is at start_brk + 8
        assert_eq!(offset_of!(StateWithTracer, tracer), 33096);
    }

    #[test]
    fn test_rv64_state_with_instret_suspender() {
        // When suspender is InstretSuspender (8 bytes), it's placed right after instret
        // for C layout compatibility (state->target_instret access)
        type StateWithSuspender = RvState<Rv64, (), InstretSuspender, NUM_REGS_I>;
        let base_size = size_of::<Rv64State>();
        let suspender_size = size_of::<InstretSuspender>();
        assert_eq!(suspender_size, 8);
        assert_eq!(size_of::<StateWithSuspender>(), base_size + suspender_size);

        // Suspender offset is right after instret (at 33048 + 8 = 33056)
        assert_eq!(offset_of!(StateWithSuspender, suspender), 33056);
    }

    #[test]
    fn test_state_new() {
        let state = Rv64State::new();
        assert_eq!(state.pc(), 0);
        assert_eq!(state.instret(), 0);
        assert!(!state.has_exited());
        assert_eq!(state.exit_code(), 0);
    }

    #[test]
    fn test_state_reset() {
        let mut state = Rv64State::new();
        state.set_pc(0x1000);
        state.instret = 100;
        state.has_exited = 1;
        state.exit_code = 42;

        state.reset();

        assert_eq!(state.pc(), 0);
        assert_eq!(state.instret(), 0);
        assert!(!state.has_exited());
        assert_eq!(state.exit_code(), 0);
    }

    #[test]
    fn test_tracer_kind() {
        assert_eq!(Rv64State::tracer_kind(), 0); // No tracer
        assert_eq!(Rv64StateWith::<PreflightTracer<Rv64>>::tracer_kind(), 1);
    }

    #[test]
    fn test_has_suspender() {
        assert!(!Rv64State::has_suspender()); // No suspender
        assert!(RvState::<Rv64, (), InstretSuspender, NUM_REGS_I>::has_suspender());
    }
}
