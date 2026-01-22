//! Platform-specific performance counter support.
//!
//! On Linux, uses the `perf_event` crate for hardware performance counters.
//! On other platforms, provides stub implementations that return `None`.

use crate::PerfCounters;

// ============================================================================
// Linux implementation
// ============================================================================

#[cfg(target_os = "linux")]
mod inner {
    use super::PerfCounters;
    use perf_event::events::Hardware;
    use perf_event::{Builder, Counter, Group};

    /// Perf counter group for in-process measurement (used by Runner).
    pub struct PerfGroup {
        group: Group,
        cycles: Counter,
        instructions: Counter,
        branches: Counter,
        branch_misses: Counter,
    }

    impl PerfGroup {
        pub fn new() -> Option<Self> {
            let mut group = Group::new().ok()?;

            let cycles = Builder::new()
                .group(&mut group)
                .kind(Hardware::CPU_CYCLES)
                .build()
                .ok()?;
            let instructions = Builder::new()
                .group(&mut group)
                .kind(Hardware::INSTRUCTIONS)
                .build()
                .ok()?;
            let branches = Builder::new()
                .group(&mut group)
                .kind(Hardware::BRANCH_INSTRUCTIONS)
                .build()
                .ok()?;
            let branch_misses = Builder::new()
                .group(&mut group)
                .kind(Hardware::BRANCH_MISSES)
                .build()
                .ok()?;

            Some(Self {
                group,
                cycles,
                instructions,
                branches,
                branch_misses,
            })
        }

        pub fn enable(&mut self) -> std::io::Result<()> {
            self.group.enable()
        }

        pub fn disable(&mut self) -> std::io::Result<()> {
            self.group.disable()
        }

        pub fn reset(&mut self) -> std::io::Result<()> {
            self.group.reset()
        }

        pub fn read(&mut self) -> Option<PerfCounters> {
            let counts = self.group.read().ok()?;
            Some(PerfCounters {
                cycles: counts.get(&self.cycles).copied(),
                instructions: counts.get(&self.instructions).copied(),
                branches: counts.get(&self.branches).copied(),
                branch_misses: counts.get(&self.branch_misses).copied(),
            })
        }
    }

    /// Individual perf counters for child process measurement (used by bench).
    /// Uses inherit(true) to track forked child processes.
    /// Note: We use individual counters instead of a group because
    /// inherit doesn't work properly with perf groups.
    pub struct HostPerfCounters {
        cycles: Counter,
        instructions: Counter,
        branches: Counter,
        branch_misses: Counter,
    }

    impl HostPerfCounters {
        pub fn new() -> Option<Self> {
            let cycles = Builder::new()
                .kind(Hardware::CPU_CYCLES)
                .inherit(true)
                .build()
                .ok()?;
            let instructions = Builder::new()
                .kind(Hardware::INSTRUCTIONS)
                .inherit(true)
                .build()
                .ok()?;
            let branches = Builder::new()
                .kind(Hardware::BRANCH_INSTRUCTIONS)
                .inherit(true)
                .build()
                .ok()?;
            let branch_misses = Builder::new()
                .kind(Hardware::BRANCH_MISSES)
                .inherit(true)
                .build()
                .ok()?;

            Some(Self {
                cycles,
                instructions,
                branches,
                branch_misses,
            })
        }

        pub fn enable(&mut self) -> std::io::Result<()> {
            self.cycles.enable()?;
            self.instructions.enable()?;
            self.branches.enable()?;
            self.branch_misses.enable()?;
            Ok(())
        }

        pub fn disable(&mut self) -> std::io::Result<()> {
            self.cycles.disable()?;
            self.instructions.disable()?;
            self.branches.disable()?;
            self.branch_misses.disable()?;
            Ok(())
        }

        pub fn read(&mut self) -> PerfCounters {
            PerfCounters {
                cycles: self.cycles.read().ok(),
                instructions: self.instructions.read().ok(),
                branches: self.branches.read().ok(),
                branch_misses: self.branch_misses.read().ok(),
            }
        }

        /// Read counters and return delta since last snapshot.
        /// This works around the issue that reset() doesn't properly
        /// clear accumulated child process counts with inherit=true.
        pub fn read_delta(&mut self, prev: &PerfCounters) -> PerfCounters {
            let curr = self.read();
            PerfCounters {
                cycles: match (curr.cycles, prev.cycles) {
                    (Some(c), Some(p)) => Some(c.saturating_sub(p)),
                    (Some(c), None) => Some(c),
                    _ => None,
                },
                instructions: match (curr.instructions, prev.instructions) {
                    (Some(c), Some(p)) => Some(c.saturating_sub(p)),
                    (Some(c), None) => Some(c),
                    _ => None,
                },
                branches: match (curr.branches, prev.branches) {
                    (Some(c), Some(p)) => Some(c.saturating_sub(p)),
                    (Some(c), None) => Some(c),
                    _ => None,
                },
                branch_misses: match (curr.branch_misses, prev.branch_misses) {
                    (Some(c), Some(p)) => Some(c.saturating_sub(p)),
                    (Some(c), None) => Some(c),
                    _ => None,
                },
            }
        }
    }
}

// ============================================================================
// Non-Linux stub implementation
// ============================================================================

#[cfg(not(target_os = "linux"))]
mod inner {
    use super::PerfCounters;

    /// Stub perf counter group (no-op on non-Linux).
    pub struct PerfGroup;

    impl PerfGroup {
        pub fn new() -> Option<Self> {
            None
        }

        pub fn enable(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        pub fn disable(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        pub fn reset(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        pub fn read(&mut self) -> Option<PerfCounters> {
            None
        }
    }

    /// Stub host perf counters (no-op on non-Linux).
    pub struct HostPerfCounters;

    impl HostPerfCounters {
        pub fn new() -> Option<Self> {
            None
        }

        pub fn enable(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        pub fn disable(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        pub fn read(&mut self) -> PerfCounters {
            PerfCounters::default()
        }

        pub fn read_delta(&mut self, _prev: &PerfCounters) -> PerfCounters {
            PerfCounters::default()
        }
    }
}

pub use inner::{HostPerfCounters, PerfGroup};
