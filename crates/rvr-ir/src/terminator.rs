//! Block terminator IR.

use crate::xlen::Xlen;

use crate::expr::Expr;

/// Branch hint for static prediction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchHint {
    None,
    Taken,
    NotTaken,
}

/// Block terminator - controls where execution goes next.
#[derive(Clone, Debug)]
pub enum Terminator<X: Xlen> {
    /// Fall through to next instruction.
    Fall {
        /// Target PC (`next_pc`). Optional for backward compatibility.
        target: Option<X::Reg>,
    },
    /// Unconditional jump to static target.
    Jump { target: X::Reg },
    /// Unconditional jump to computed address.
    JumpDyn {
        addr: Expr<X>,
        /// Known possible targets (for dispatch table).
        resolved: Option<Vec<X::Reg>>,
    },
    /// Conditional branch.
    Branch {
        cond: Expr<X>,
        /// Branch taken target.
        target: X::Reg,
        /// Fall-through target (`next_pc`).
        fall: Option<X::Reg>,
        hint: BranchHint,
    },
    /// Exit program with code.
    Exit { code: Expr<X> },
    /// Trap (illegal instruction, etc.).
    Trap { message: String },
}

impl<X: Xlen> Default for Terminator<X> {
    fn default() -> Self {
        Self::Fall { target: None }
    }
}

impl<X: Xlen> Terminator<X> {
    /// Create a static jump terminator.
    pub const fn jump(target: X::Reg) -> Self {
        Self::Jump { target }
    }

    /// Create a dynamic jump terminator.
    pub const fn jump_dyn(addr: Expr<X>) -> Self {
        Self::JumpDyn {
            addr,
            resolved: None,
        }
    }

    /// Create a fall-through terminator with explicit target.
    pub const fn fall(target: X::Reg) -> Self {
        Self::Fall {
            target: Some(target),
        }
    }

    /// Create a conditional branch terminator.
    pub const fn branch(cond: Expr<X>, target: X::Reg) -> Self {
        Self::Branch {
            cond,
            target,
            fall: None,
            hint: BranchHint::None,
        }
    }

    /// Create a conditional branch terminator with fall-through target.
    pub const fn branch_with_fall(cond: Expr<X>, target: X::Reg, fall: X::Reg) -> Self {
        Self::Branch {
            cond,
            target,
            fall: Some(fall),
            hint: BranchHint::None,
        }
    }

    /// Create an exit terminator.
    pub const fn exit(code: Expr<X>) -> Self {
        Self::Exit { code }
    }

    /// Create a trap terminator.
    #[must_use]
    pub fn trap(message: &str) -> Self {
        Self::Trap {
            message: message.to_string(),
        }
    }

    /// Check if this terminator is a fall-through.
    pub const fn is_fall(&self) -> bool {
        matches!(self, Self::Fall { .. })
    }

    /// Check if this terminator is any kind of jump.
    pub const fn is_jump(&self) -> bool {
        matches!(self, Self::Jump { .. } | Self::JumpDyn { .. })
    }

    /// Check if this terminator is a static jump.
    pub const fn is_static_jump(&self) -> bool {
        matches!(self, Self::Jump { .. })
    }

    /// Check if this terminator is a dynamic jump.
    pub const fn is_dyn_jump(&self) -> bool {
        matches!(self, Self::JumpDyn { .. })
    }

    /// Check if this terminator is a branch.
    pub const fn is_branch(&self) -> bool {
        matches!(self, Self::Branch { .. })
    }

    /// Get static targets (if any).
    pub fn static_targets(&self) -> Vec<X::Reg> {
        match self {
            Self::Jump { target } | Self::Branch { target, .. } => vec![*target],
            Self::JumpDyn {
                resolved: Some(targets),
                ..
            } => targets.clone(),
            _ => Vec::new(),
        }
    }

    /// Check if this terminator is any kind of control flow (not fall-through).
    pub const fn is_control_flow(&self) -> bool {
        !matches!(self, Self::Fall { .. })
    }

    /// Get the fall-through target PC, if any.
    pub const fn fall_target(&self) -> Option<X::Reg> {
        match self {
            Self::Fall { target } => *target,
            Self::Branch { fall, .. } => *fall,
            _ => None,
        }
    }
}
