//! Immediate encoding/decoding helpers for RISC-V instructions.

/// Decode I-type immediate (bits [31:20] sign-extended).
#[inline]
pub fn decode_i_imm(instr: u32) -> i32 {
    (instr as i32) >> 20
}

/// Decode S-type immediate (bits [31:25] | [11:7] sign-extended).
#[inline]
pub fn decode_s_imm(instr: u32) -> i32 {
    let imm11_5 = (instr >> 25) & 0x7F;
    let imm4_0 = (instr >> 7) & 0x1F;
    let imm = (imm11_5 << 5) | imm4_0;
    // Sign extend from 12 bits
    ((imm as i32) << 20) >> 20
}

/// Decode B-type immediate (bits [31] | [7] | [30:25] | [11:8] sign-extended, << 1).
#[inline]
pub fn decode_b_imm(instr: u32) -> i32 {
    let imm12 = (instr >> 31) & 0x1;
    let imm11 = (instr >> 7) & 0x1;
    let imm10_5 = (instr >> 25) & 0x3F;
    let imm4_1 = (instr >> 8) & 0xF;
    let imm = (imm12 << 12) | (imm11 << 11) | (imm10_5 << 5) | (imm4_1 << 1);
    // Sign extend from 13 bits
    ((imm as i32) << 19) >> 19
}

/// Decode U-type immediate (bits [31:12] << 12).
#[inline]
pub fn decode_u_imm(instr: u32) -> i32 {
    (instr & 0xFFFFF000) as i32
}

/// Decode J-type immediate (bits [31] | [19:12] | [20] | [30:21] sign-extended, << 1).
#[inline]
pub fn decode_j_imm(instr: u32) -> i32 {
    let imm20 = (instr >> 31) & 0x1;
    let imm19_12 = (instr >> 12) & 0xFF;
    let imm11 = (instr >> 20) & 0x1;
    let imm10_1 = (instr >> 21) & 0x3FF;
    let imm = (imm20 << 20) | (imm19_12 << 12) | (imm11 << 11) | (imm10_1 << 1);
    // Sign extend from 21 bits
    ((imm as i32) << 11) >> 11
}

/// Extract rd field (bits [11:7]).
#[inline]
pub fn decode_rd(instr: u32) -> u8 {
    ((instr >> 7) & 0x1F) as u8
}

/// Extract rs1 field (bits [19:15]).
#[inline]
pub fn decode_rs1(instr: u32) -> u8 {
    ((instr >> 15) & 0x1F) as u8
}

/// Extract rs2 field (bits [24:20]).
#[inline]
pub fn decode_rs2(instr: u32) -> u8 {
    ((instr >> 20) & 0x1F) as u8
}

/// Extract funct3 field (bits [14:12]).
#[inline]
pub fn decode_funct3(instr: u32) -> u8 {
    ((instr >> 12) & 0x7) as u8
}

/// Extract funct7 field (bits [31:25]).
#[inline]
pub fn decode_funct7(instr: u32) -> u8 {
    ((instr >> 25) & 0x7F) as u8
}

/// Extract opcode field (bits [6:0]).
#[inline]
pub fn decode_opcode(instr: u32) -> u8 {
    (instr & 0x7F) as u8
}

/// Sign extend from 8 bits to 64 bits.
#[inline]
pub fn sign_extend_8(val: u8) -> i64 {
    val as i8 as i64
}

/// Sign extend from 16 bits to 64 bits.
#[inline]
pub fn sign_extend_16(val: u16) -> i64 {
    val as i16 as i64
}

/// Sign extend from 32 bits to 64 bits.
#[inline]
pub fn sign_extend_32(val: u32) -> i64 {
    val as i32 as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_i_imm() {
        // ADDI x1, x0, 1 -> imm = 1
        let instr = 0x00100093;
        assert_eq!(decode_i_imm(instr), 1);

        // ADDI x1, x0, -1 -> imm = -1
        let instr = 0xFFF00093;
        assert_eq!(decode_i_imm(instr), -1);
    }

    #[test]
    fn test_decode_b_imm() {
        // BEQ with offset 8 (forward branch)
        let instr = 0x00000463; // beq x0, x0, 8
        assert_eq!(decode_b_imm(instr), 8);
    }

    #[test]
    fn test_decode_j_imm() {
        // JAL with offset 0
        let instr = 0x0000006F;
        assert_eq!(decode_j_imm(instr), 0);
    }

    #[test]
    fn test_field_extraction() {
        // ADDI x1, x2, 100 -> rd=1, rs1=2
        let instr = 0x06410093;
        assert_eq!(decode_rd(instr), 1);
        assert_eq!(decode_rs1(instr), 2);
        assert_eq!(decode_opcode(instr), 0x13);
    }
}
