//! Syscall/ECALL handling for RISC-V.
//!
//! Provides a small, table-driven mechanism for lowering ECALL to IR.
//! The default handler matches riscv-tests semantics (exit with a0).
//! Linux-style syscalls are handled via a syscall table that dispatches
//! to runtime C helpers (rv_sys_*).
//!
//! # Usage
//!
//! ```ignore
//! use rvr_isa::{ExtensionRegistry, Rv64};
//! use rvr_isa::syscalls::{LinuxHandler, SyscallAbi};
//!
//! let registry = ExtensionRegistry::<Rv64>::standard()
//!     .with_syscall_handler(LinuxHandler::new(SyscallAbi::Standard));
//! ```

mod baremetal;
mod linux;
mod table;

pub use baremetal::{BareMetalHandler, RiscvTestsHandler};
pub use linux::{LinuxHandler, syscall_nr};
pub use table::{SyscallAbi, SyscallAction, SyscallEntry, SyscallHandler, SyscallTable};
