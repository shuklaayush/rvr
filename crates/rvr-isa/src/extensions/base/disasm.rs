use super::*;

pub(super) fn format_instr(mnemonic: &str, args: &InstrArgs) -> String {
    match args {
        InstrArgs::R { rd, rs1, rs2 } => {
            format!(
                "{} {}, {}, {}",
                mnemonic,
                reg_name(*rd),
                reg_name(*rs1),
                reg_name(*rs2)
            )
        }
        InstrArgs::I { rd, rs1, imm } => {
            if mnemonic.starts_with('l') && mnemonic != "lui" {
                format!(
                    "{} {}, {}({})",
                    mnemonic,
                    reg_name(*rd),
                    imm,
                    reg_name(*rs1)
                )
            } else {
                format!(
                    "{} {}, {}, {}",
                    mnemonic,
                    reg_name(*rd),
                    reg_name(*rs1),
                    imm
                )
            }
        }
        InstrArgs::S { rs1, rs2, imm } => {
            format!(
                "{} {}, {}({})",
                mnemonic,
                reg_name(*rs2),
                imm,
                reg_name(*rs1)
            )
        }
        InstrArgs::B { rs1, rs2, imm } => {
            format!(
                "{} {}, {}, {}",
                mnemonic,
                reg_name(*rs1),
                reg_name(*rs2),
                imm
            )
        }
        InstrArgs::U { rd, imm } => {
            format!("{} {}, {:#x}", mnemonic, reg_name(*rd), (*imm as u32) >> 12)
        }
        InstrArgs::J { rd, imm } => {
            format!("{} {}, {}", mnemonic, reg_name(*rd), imm)
        }
        InstrArgs::None => mnemonic.to_string(),
        _ => format!("{} <?>", mnemonic),
    }
}
