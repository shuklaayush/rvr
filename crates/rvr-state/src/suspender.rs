//! Suspender state types for cooperative execution.
//!
//! Suspenders allow the VM to pause execution at specific points,
//! typically based on instruction count (instret).
//!
//! When no suspension is needed, use `()` which is a ZST and adds nothing
//! to the struct layout. For instret-based suspension, use `InstretSuspender`.

/// Marker trait for FFI-safe suspender state.
///
/// Types implementing this trait can be embedded in `RvState` and must:
/// - Have `#[repr(C)]` layout (or be ZST)
/// - Match the corresponding C struct exactly
pub trait SuspenderState: Default + Copy {
    /// Whether this suspender adds fields to the state struct.
    const HAS_FIELDS: bool;
}

// No suspender - zero-sized type, adds nothing to struct
impl SuspenderState for () {
    const HAS_FIELDS: bool = false;
}

/// Instret-based suspender state - suspends when instret >= target.
///
/// Matches C struct field:
/// ```c
/// uint64_t target_instret;
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InstretSuspender {
    /// Target instruction count for suspension.
    pub target_instret: u64,
}

impl SuspenderState for InstretSuspender {
    const HAS_FIELDS: bool = true;
}

impl InstretSuspender {
    /// Create a new suspender with the given target.
    pub fn new(target_instret: u64) -> Self {
        Self { target_instret }
    }

    /// Check if execution should suspend.
    #[inline]
    pub fn should_suspend(&self, instret: u64) -> bool {
        instret >= self.target_instret
    }

    /// Set target instret.
    #[inline]
    pub fn set_target(&mut self, target: u64) {
        self.target_instret = target;
    }

    /// Disable suspension by setting target to max.
    #[inline]
    pub fn disable(&mut self) {
        self.target_instret = u64::MAX;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_unit_is_zst() {
        assert_eq!(size_of::<()>(), 0);
        assert!(!<() as SuspenderState>::HAS_FIELDS);
    }

    #[test]
    fn test_instret_suspender_layout() {
        // 8 (u64) = 8 bytes
        assert_eq!(size_of::<InstretSuspender>(), 8);
        assert!(InstretSuspender::HAS_FIELDS);
    }

    #[test]
    fn test_instret_suspender_logic() {
        let mut s = InstretSuspender::new(100);
        assert!(!s.should_suspend(50));
        assert!(!s.should_suspend(99));
        assert!(s.should_suspend(100));
        assert!(s.should_suspend(200));

        s.disable();
        assert!(!s.should_suspend(u64::MAX - 1));
    }
}
