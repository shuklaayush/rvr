use rvr_isa::{
    DecodedInstr, InstrArgs, OP_ADD, OP_ADDI, OP_AUIPC, OP_BEQ, OP_BGE, OP_BGEU, OP_BLT, OP_BLTU,
    OP_BNE, OP_C_ADD, OP_C_ADDI, OP_C_ADDI4SPN, OP_C_ADDI16SP, OP_C_BEQZ, OP_C_BNEZ, OP_C_J,
    OP_C_JAL, OP_C_JALR, OP_C_JR, OP_C_LD, OP_C_LDSP, OP_C_LI, OP_C_LUI, OP_C_LW, OP_C_LWSP,
    OP_C_MV, OP_JAL, OP_JALR, OP_LB, OP_LBU, OP_LD, OP_LH, OP_LHU, OP_LUI, OP_LW, OP_LWU, Xlen,
};

use super::{MAX_VALUES, NUM_REGS, extract_written_reg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueKind {
    Unknown,
    Constant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RegisterValue {
    kind: ValueKind,
    pub(super) values: Vec<u64>,
}

impl RegisterValue {
    pub(super) fn unknown() -> Self {
        Self {
            kind: ValueKind::Unknown,
            values: Vec::new(),
        }
    }

    pub(super) fn constant(value: u64) -> Self {
        Self {
            kind: ValueKind::Constant,
            values: vec![value],
        }
    }

    pub(super) fn is_constant(&self) -> bool {
        self.kind == ValueKind::Constant
    }

    pub(super) fn add_value(&mut self, value: u64) {
        if self.kind != ValueKind::Constant {
            return;
        }

        match self.values.binary_search(&value) {
            Ok(_) => {}
            Err(idx) => {
                if self.values.len() >= MAX_VALUES {
                    self.kind = ValueKind::Unknown;
                    self.values.clear();
                } else {
                    self.values.insert(idx, value);
                }
            }
        }
    }

    pub(super) fn merge(&self, other: &Self) -> Self {
        if self.kind == ValueKind::Unknown || other.kind == ValueKind::Unknown {
            return Self::unknown();
        }

        let mut merged = Vec::with_capacity(self.values.len() + other.values.len());
        let mut i = 0;
        let mut j = 0;

        while i < self.values.len() && j < other.values.len() {
            let a = self.values[i];
            let b = other.values[j];
            if a == b {
                merged.push(a);
                i += 1;
                j += 1;
            } else if a < b {
                merged.push(a);
                i += 1;
            } else {
                merged.push(b);
                j += 1;
            }

            if merged.len() > MAX_VALUES {
                return Self::unknown();
            }
        }

        while i < self.values.len() {
            merged.push(self.values[i]);
            i += 1;
            if merged.len() > MAX_VALUES {
                return Self::unknown();
            }
        }

        while j < other.values.len() {
            merged.push(other.values[j]);
            j += 1;
            if merged.len() > MAX_VALUES {
                return Self::unknown();
            }
        }

        Self {
            kind: ValueKind::Constant,
            values: merged,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct RegisterState {
    regs: [RegisterValue; NUM_REGS],
}

impl RegisterState {
    pub(super) fn new() -> Self {
        let mut regs = std::array::from_fn(|_| RegisterValue::unknown());
        regs[0] = RegisterValue::constant(0);
        Self { regs }
    }

    pub(super) fn get(&self, reg: u8) -> RegisterValue {
        let idx = reg as usize;
        if idx >= NUM_REGS {
            return RegisterValue::unknown();
        }
        if idx == 0 {
            return RegisterValue::constant(0);
        }
        self.regs[idx].clone()
    }

    pub(super) fn get_ref(&self, reg: u8) -> &RegisterValue {
        let idx = reg as usize;
        if idx >= NUM_REGS {
            return &self.regs[0];
        }
        &self.regs[idx]
    }

    pub(super) fn set(&mut self, reg: u8, value: RegisterValue) {
        let idx = reg as usize;
        if idx == 0 || idx >= NUM_REGS {
            return;
        }
        self.regs[idx] = value;
    }

    pub(super) fn set_constant(&mut self, reg: u8, value: u64) {
        self.set(reg, RegisterValue::constant(value));
    }

    pub(super) fn set_unknown(&mut self, reg: u8) {
        self.set(reg, RegisterValue::unknown());
    }

    pub(super) fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for idx in 1..NUM_REGS {
            let merged = self.regs[idx].merge(&other.regs[idx]);
            if merged != self.regs[idx] {
                self.regs[idx] = merged;
                changed = true;
            }
        }
        changed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InstrKind {
    Unknown,
    Lui,
    Auipc,
    Addi,
    Add,
    Move,
    Jal,
    Jalr,
    Load,
    Branch,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DecodedInstruction {
    pub(super) kind: InstrKind,
    pub(super) rd: Option<u8>,
    pub(super) rs1: Option<u8>,
    pub(super) rs2: Option<u8>,
    pub(super) imm: i32,
    pub(super) width: u8,
    pub(super) is_unsigned: bool,
}

impl DecodedInstruction {
    pub(super) fn unknown() -> Self {
        Self {
            kind: InstrKind::Unknown,
            rd: None,
            rs1: None,
            rs2: None,
            imm: 0,
            width: 0,
            is_unsigned: false,
        }
    }

    pub(super) fn from_instr<X: Xlen>(instr: &DecodedInstr<X>) -> Self {
        let opid = instr.opid;
        match opid {
            OP_LUI | OP_C_LUI => match instr.args.clone() {
                InstrArgs::U { rd, imm } => Self {
                    kind: InstrKind::Lui,
                    rd: Some(rd),
                    rs1: None,
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_AUIPC => match instr.args.clone() {
                InstrArgs::U { rd, imm } => Self {
                    kind: InstrKind::Auipc,
                    rd: Some(rd),
                    rs1: None,
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_ADDI | OP_C_ADDI | OP_C_ADDI16SP | OP_C_ADDI4SPN | OP_C_LI => {
                match instr.args.clone() {
                    InstrArgs::I { rd, rs1, imm } => Self {
                        kind: InstrKind::Addi,
                        rd: Some(rd),
                        rs1: Some(rs1),
                        rs2: None,
                        imm,
                        width: 0,
                        is_unsigned: false,
                    },
                    _ => Self::unknown(),
                }
            }
            OP_ADD | OP_C_ADD => match instr.args.clone() {
                InstrArgs::R { rd, rs1, rs2 } => Self {
                    kind: InstrKind::Add,
                    rd: Some(rd),
                    rs1: Some(rs1),
                    rs2: Some(rs2),
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_C_MV => match instr.args.clone() {
                InstrArgs::R { rd, rs2, .. } => Self {
                    kind: InstrKind::Move,
                    rd: Some(rd),
                    rs1: Some(rs2),
                    rs2: None,
                    imm: 0,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_JAL | OP_C_J | OP_C_JAL => match instr.args.clone() {
                InstrArgs::J { rd, imm } => Self {
                    kind: InstrKind::Jal,
                    rd: Some(rd),
                    rs1: None,
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_JALR | OP_C_JR | OP_C_JALR => match instr.args.clone() {
                InstrArgs::I { rd, rs1, imm } => Self {
                    kind: InstrKind::Jalr,
                    rd: Some(rd),
                    rs1: Some(rs1),
                    rs2: None,
                    imm,
                    width: 0,
                    is_unsigned: false,
                },
                _ => Self::unknown(),
            },
            OP_LB | OP_LBU | OP_LH | OP_LHU | OP_LW | OP_LWU | OP_LD | OP_C_LW | OP_C_LWSP
            | OP_C_LD | OP_C_LDSP => match instr.args.clone() {
                InstrArgs::I { rd, rs1, imm } => {
                    let (width, is_unsigned) = match opid {
                        OP_LB => (1, false),
                        OP_LBU => (1, true),
                        OP_LH => (2, false),
                        OP_LHU => (2, true),
                        OP_LW => (4, false),
                        OP_LWU => (4, true),
                        OP_LD => (8, false),
                        OP_C_LW | OP_C_LWSP => (4, false),
                        OP_C_LD | OP_C_LDSP => (8, false),
                        _ => (0, false),
                    };
                    Self {
                        kind: InstrKind::Load,
                        rd: Some(rd),
                        rs1: Some(rs1),
                        rs2: None,
                        imm,
                        width,
                        is_unsigned,
                    }
                }
                _ => Self::unknown(),
            },
            OP_BEQ | OP_BNE | OP_BLT | OP_BGE | OP_BLTU | OP_BGEU | OP_C_BEQZ | OP_C_BNEZ => {
                match instr.args.clone() {
                    InstrArgs::B { rs1, rs2, imm } => Self {
                        kind: InstrKind::Branch,
                        rd: None,
                        rs1: Some(rs1),
                        rs2: Some(rs2),
                        imm,
                        width: 0,
                        is_unsigned: false,
                    },
                    _ => Self::unknown(),
                }
            }
            _ => {
                let rd = extract_written_reg(&instr.args);
                let mut decoded = Self::unknown();
                decoded.rd = rd;
                decoded
            }
        }
    }

    pub(super) fn is_control_flow(&self) -> bool {
        matches!(
            self.kind,
            InstrKind::Jal | InstrKind::Jalr | InstrKind::Branch
        )
    }

    pub(super) fn is_static_call(&self) -> bool {
        self.kind == InstrKind::Jal && self.rd != Some(0)
    }

    pub(super) fn is_call(&self) -> bool {
        match self.kind {
            InstrKind::Jal | InstrKind::Jalr => self.rd != Some(0),
            _ => false,
        }
    }

    pub(super) fn is_return(&self) -> bool {
        self.kind == InstrKind::Jalr && self.rd == Some(0) && self.rs1 == Some(1)
    }

    pub(super) fn is_indirect_jump(&self) -> bool {
        self.kind == InstrKind::Jalr && self.rd == Some(0) && self.rs1 != Some(1)
    }
}
