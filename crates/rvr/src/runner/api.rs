//! Library API loading and types.

use std::ffi::c_void;

use libloading::os::unix::{Library, Symbol};
use tracing::error;

use super::RunError;

/// C API - only the execution function is required.
pub type RvExecuteFrom = unsafe extern "C" fn(*mut c_void, u64) -> i32;

/// Fixed address configuration loaded from library.
#[derive(Clone, Copy, Debug)]
pub struct FixedAddresses {
    pub state_addr: u64,
    pub memory_addr: u64,
}

/// Minimal API from the generated C code.
#[derive(Clone, Copy)]
pub struct RvApi {
    pub execute_from: RvExecuteFrom,
    pub tracer_kind: u32,
    pub export_functions: bool,
    pub instret_mode: u32,
    pub fixed_addresses: Option<FixedAddresses>,
}

impl RvApi {
    pub unsafe fn load(lib: &Library) -> Result<Self, RunError> {
        unsafe {
            // Load fixed addresses if present
            let fixed_addresses = match (
                load_data_symbol_u64(lib, b"RV_FIXED_STATE_ADDR"),
                load_data_symbol_u64(lib, b"RV_FIXED_MEMORY_ADDR"),
            ) {
                (Some(state_addr), Some(memory_addr)) => Some(FixedAddresses {
                    state_addr,
                    memory_addr,
                }),
                _ => None,
            };

            Ok(Self {
                execute_from: load_symbol(lib, b"rv_execute_from", "rv_execute_from")?,
                tracer_kind: load_data_symbol(lib, b"RV_TRACER_KIND").unwrap_or(0),
                export_functions: load_data_symbol(lib, b"RV_EXPORT_FUNCTIONS").unwrap_or(0) != 0,
                instret_mode: load_data_symbol(lib, b"RV_INSTRET_MODE").unwrap_or(1), // Default to Count
                fixed_addresses,
            })
        }
    }

    /// Check if the library supports suspend mode (for single-stepping).
    pub const fn supports_suspend(&self) -> bool {
        // Suspend (2) or PerInstruction (3) mode
        self.instret_mode >= 2
    }
}

pub unsafe fn load_symbol<T: Copy>(
    lib: &Library,
    symbol: &'static [u8],
    label: &'static str,
) -> Result<T, RunError> {
    unsafe {
        let sym: Symbol<T> = lib.get(symbol).map_err(|e| {
            error!(symbol = label, "symbol not found in library");
            RunError::SymbolNotFound(label.to_string(), e)
        })?;
        Ok(*sym)
    }
}

pub unsafe fn load_data_symbol(lib: &Library, symbol: &'static [u8]) -> Option<u32> {
    unsafe {
        let sym: Symbol<*const u32> = lib.get(symbol).ok()?;
        Some(**sym)
    }
}

pub unsafe fn load_data_symbol_u64(lib: &Library, symbol: &'static [u8]) -> Option<u64> {
    unsafe {
        let sym: Symbol<*const u64> = lib.get(symbol).ok()?;
        Some(**sym)
    }
}

/// Tracer kind matches `RV_TRACER_KIND` in generated C code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TracerKind {
    None,
    Preflight,
    Stats,
    Ffi,
    Dynamic,
    Debug,
    Spike,
    Diff,
    BufferedDiff,
}

impl TracerKind {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Preflight,
            2 => Self::Stats,
            3 => Self::Ffi,
            4 => Self::Dynamic,
            5 => Self::Debug,
            6 => Self::Spike,
            7 => Self::Diff,
            8 => Self::BufferedDiff,
            _ => Self::None,
        }
    }
}

/// Instret mode matches `RV_INSTRET_MODE` in generated C code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstretMode {
    /// No instruction counting.
    Off,
    /// Count instructions but don't suspend.
    Count,
    /// Count instructions and suspend at limit (block boundaries).
    Suspend,
    /// Count instructions and suspend at limit (per instruction).
    PerInstruction,
}

impl InstretMode {
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Off,
            2 => Self::Suspend,
            3 => Self::PerInstruction,
            _ => Self::Count, // Default to Count (1)
        }
    }

    /// True if the mode supports suspension (`Suspend` or `PerInstruction`).
    pub const fn is_suspend(self) -> bool {
        matches!(self, Self::Suspend | Self::PerInstruction)
    }
}
