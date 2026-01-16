//! Register value tracking for CFG analysis.

const MAX_VALUES: usize = 16;

/// Tracked register value - either unknown or a set of possible constant values.
#[derive(Clone, Debug)]
pub enum RegisterValue {
    /// Value is unknown (too many possibilities or computed from unknown).
    Unknown,
    /// Value is one of these constant values (sorted, deduplicated).
    Constant(Vec<u64>),
}

impl Default for RegisterValue {
    fn default() -> Self {
        Self::Unknown
    }
}

impl RegisterValue {
    /// Create an unknown value.
    pub fn unknown() -> Self {
        Self::Unknown
    }

    /// Create a single constant value.
    pub fn constant(val: u64) -> Self {
        Self::Constant(vec![val])
    }

    /// Check if value is a known constant.
    pub fn is_constant(&self) -> bool {
        matches!(self, Self::Constant(_))
    }

    /// Get constant values if known.
    pub fn values(&self) -> Option<&[u64]> {
        match self {
            Self::Constant(v) => Some(v),
            Self::Unknown => None,
        }
    }

    /// Add a value to the set (maintains sorted order).
    pub fn add_value(&mut self, val: u64) {
        if let Self::Constant(ref mut values) = self {
            // Binary search for insertion point
            match values.binary_search(&val) {
                Ok(_) => {} // Already exists
                Err(pos) => {
                    if values.len() >= MAX_VALUES {
                        // Too many values, become unknown
                        *self = Self::Unknown;
                    } else {
                        values.insert(pos, val);
                    }
                }
            }
        }
    }

    /// Merge two values, returning the union.
    pub fn merge(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Constant(a), Self::Constant(b)) => {
                // Sorted merge
                let mut merged = Vec::with_capacity(a.len() + b.len());
                let mut i = 0;
                let mut j = 0;
                while i < a.len() && j < b.len() {
                    if a[i] < b[j] {
                        merged.push(a[i]);
                        i += 1;
                    } else if a[i] > b[j] {
                        merged.push(b[j]);
                        j += 1;
                    } else {
                        // Equal - only add once
                        merged.push(a[i]);
                        i += 1;
                        j += 1;
                    }
                    if merged.len() > MAX_VALUES {
                        return Self::Unknown;
                    }
                }
                while i < a.len() {
                    merged.push(a[i]);
                    i += 1;
                    if merged.len() > MAX_VALUES {
                        return Self::Unknown;
                    }
                }
                while j < b.len() {
                    merged.push(b[j]);
                    j += 1;
                    if merged.len() > MAX_VALUES {
                        return Self::Unknown;
                    }
                }
                Self::Constant(merged)
            }
        }
    }
}

impl PartialEq for RegisterValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unknown, Self::Unknown) => true,
            (Self::Constant(a), Self::Constant(b)) => a == b,
            _ => false,
        }
    }
}

/// Register state during CFG analysis.
#[derive(Clone, Debug)]
pub struct RegisterState {
    regs: [RegisterValue; 32],
}

impl Default for RegisterState {
    fn default() -> Self {
        Self::new()
    }
}

impl RegisterState {
    /// Create new state with x0 = 0 and all others unknown.
    pub fn new() -> Self {
        let mut regs: [RegisterValue; 32] = Default::default();
        regs[0] = RegisterValue::constant(0);
        Self { regs }
    }

    /// Get register value.
    pub fn get(&self, reg: usize) -> &RegisterValue {
        if reg >= 32 {
            return &RegisterValue::Unknown;
        }
        &self.regs[reg]
    }

    /// Set register value.
    pub fn set(&mut self, reg: usize, value: RegisterValue) {
        if reg > 0 && reg < 32 {
            self.regs[reg] = value;
        }
    }

    /// Set register to a constant.
    pub fn set_constant(&mut self, reg: usize, val: u64) {
        self.set(reg, RegisterValue::constant(val));
    }

    /// Set register to unknown.
    pub fn set_unknown(&mut self, reg: usize) {
        self.set(reg, RegisterValue::Unknown);
    }

    /// Merge another state into this one. Returns true if anything changed.
    pub fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for i in 1..32 {
            let merged = self.regs[i].merge(&other.regs[i]);
            if merged != self.regs[i] {
                self.regs[i] = merged;
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_value_constant() {
        let v = RegisterValue::constant(42);
        assert!(v.is_constant());
        assert_eq!(v.values(), Some(&[42][..]));
    }

    #[test]
    fn test_register_value_merge() {
        let a = RegisterValue::constant(1);
        let b = RegisterValue::constant(2);
        let merged = a.merge(&b);
        assert_eq!(merged.values(), Some(&[1, 2][..]));
    }

    #[test]
    fn test_register_state() {
        let mut state = RegisterState::new();
        assert!(state.get(0).is_constant()); // x0 = 0
        assert!(!state.get(1).is_constant()); // x1 unknown

        state.set_constant(1, 100);
        assert_eq!(state.get(1).values(), Some(&[100][..]));
    }
}
