//! Block terminator IR.

use rvr_isa::Xlen;

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
    Fall,
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
        target: X::Reg,
        hint: BranchHint,
    },
    /// Exit program with code.
    Exit { code: Expr<X> },
    /// Trap (illegal instruction, etc.).
    Trap { message: String },
}

impl<X: Xlen> Default for Terminator<X> {
    fn default() -> Self {
        Self::Fall
    }
}

impl<X: Xlen> Terminator<X> {
    /// Create a static jump terminator.
    pub fn jump(target: X::Reg) -> Self {
        Self::Jump { target }
    }

    /// Create a dynamic jump terminator.
    pub fn jump_dyn(addr: Expr<X>) -> Self {
        Self::JumpDyn {
            addr,
            resolved: None,
        }
    }

    /// Create a conditional branch terminator.
    pub fn branch(cond: Expr<X>, target: X::Reg) -> Self {
        Self::Branch {
            cond,
            target,
            hint: BranchHint::None,
        }
    }

    /// Create an exit terminator.
    pub fn exit(code: Expr<X>) -> Self {
        Self::Exit { code }
    }

    /// Create a trap terminator.
    pub fn trap(message: &str) -> Self {
        Self::Trap {
            message: message.to_string(),
        }
    }

    /// Check if this terminator is a fall-through.
    pub fn is_fall(&self) -> bool {
        matches!(self, Self::Fall)
    }

    /// Check if this terminator is any kind of jump.
    pub fn is_jump(&self) -> bool {
        matches!(self, Self::Jump { .. } | Self::JumpDyn { .. })
    }

    /// Check if this terminator is a static jump.
    pub fn is_static_jump(&self) -> bool {
        matches!(self, Self::Jump { .. })
    }

    /// Check if this terminator is a dynamic jump.
    pub fn is_dyn_jump(&self) -> bool {
        matches!(self, Self::JumpDyn { .. })
    }

    /// Check if this terminator is a branch.
    pub fn is_branch(&self) -> bool {
        matches!(self, Self::Branch { .. })
    }

    /// Get static targets (if any).
    pub fn static_targets(&self) -> Vec<X::Reg> {
        match self {
            Self::Jump { target } => vec![*target],
            Self::Branch { target, .. } => vec![*target],
            Self::JumpDyn { resolved: Some(targets), .. } => targets.clone(),
            _ => Vec::new(),
        }
    }
}
