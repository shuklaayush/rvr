use super::{InstrArgs, reg_name};

pub(super) fn format_c_instr(mnemonic: &str, args: &InstrArgs, opid: crate::OpId) -> String {
    match args {
        InstrArgs::R { rd, rs1: _, rs2 } => {
            // C.MV and C.ADD use the same format
            let _ = opid; // silence unused warning
            format!("{} {}, {}", mnemonic, reg_name(*rd), reg_name(*rs2))
        }
        InstrArgs::I { rd, rs1: _, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rd), imm)
        }
        InstrArgs::S { rs1: _, rs2, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rs2), imm)
        }
        InstrArgs::U { rd, imm } => {
            format!(
                "{} {}, {:#x}",
                mnemonic,
                reg_name(*rd),
                imm.cast_unsigned() >> 12
            )
        }
        InstrArgs::J { rd: _, imm } => {
            format!("{mnemonic} {imm}")
        }
        InstrArgs::B { rs1, rs2: _, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rs1), imm)
        }
        InstrArgs::None => mnemonic.to_string(),
        _ => format!("{mnemonic} <?>"),
    }
}

// Compressed immediate decoders
