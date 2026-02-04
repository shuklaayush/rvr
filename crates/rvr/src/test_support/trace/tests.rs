use super::*;

#[test]
fn test_parse_trace_entry() {
    let line = "core   0: 3 0x0000000080000050 (0x00000093) x1 0x0000000000000000";
    let entry = TraceEntry::parse(line).unwrap();

    assert_eq!(entry.pc, 0x8000_0050);
    assert_eq!(entry.opcode, 0x0000_0093);
    assert_eq!(entry.rd, Some(1));
    assert_eq!(entry.rd_value, Some(0));
    assert_eq!(entry.mem_addr, None);
}

#[test]
fn test_parse_trace_entry_with_mem() {
    let line =
        "core   0: 3 0x000000008000010c (0x0182b283) x5 0x0000000080000000 mem 0x0000000000001018";
    let entry = TraceEntry::parse(line).unwrap();

    assert_eq!(entry.pc, 0x8000_010c);
    assert_eq!(entry.opcode, 0x0182_b283);
    assert_eq!(entry.rd, Some(5));
    assert_eq!(entry.rd_value, Some(0x8000_0000));
    assert_eq!(entry.mem_addr, Some(0x1018));
}

#[test]
fn test_parse_trace_entry_with_mem_value() {
    // Spike can include memory value for stores
    let line = "core   0: 3 0x80000040 (0xfc3f2223) mem 0x80001000 0x00000001";
    let entry = TraceEntry::parse(line).unwrap();

    assert_eq!(entry.pc, 0x8000_0040);
    assert_eq!(entry.opcode, 0xfc3f_2223);
    assert_eq!(entry.mem_addr, Some(0x8000_1000));
    // Value after mem addr is ignored (we only track address)
}

#[test]
fn test_parse_trace_entry_no_reg() {
    let line = "core   0: 3 0x0000000080000000 (0x0500006f)";
    let entry = TraceEntry::parse(line).unwrap();

    assert_eq!(entry.pc, 0x8000_0000);
    assert_eq!(entry.opcode, 0x0500_006f);
    assert_eq!(entry.rd, None);
    assert_eq!(entry.rd_value, None);
}

#[test]
fn test_parse_trace_entry_with_csr() {
    // Spike logs CSR writes with cNNN_name format
    let line = "core   0: 3 0x800000dc (0x30529073) c773_mtvec 0x00000000800000e4";
    let entry = TraceEntry::parse(line).unwrap();

    assert_eq!(entry.pc, 0x8000_00dc);
    assert_eq!(entry.opcode, 0x3052_9073);
    // CSR write is not parsed as xN, so rd should be None
    assert_eq!(entry.rd, None);
}

#[test]
fn test_parse_trace_entry_priv_level_0() {
    // Different privilege level format
    let line = "core   0: 0 0x80000200 (0x00c70733) x14 0x0000000000000337";
    let entry = TraceEntry::parse(line).unwrap();

    assert_eq!(entry.pc, 0x8000_0200);
    assert_eq!(entry.opcode, 0x00c7_0733);
    assert_eq!(entry.rd, Some(14));
    assert_eq!(entry.rd_value, Some(0x337));
}

#[test]
fn test_compare_traces_match() {
    let traces = vec![
        TraceEntry {
            pc: 0x8000_0000,
            opcode: 0x0500_006f,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
        TraceEntry {
            pc: 0x8000_0050,
            opcode: 0x0000_0093,
            rd: Some(1),
            rd_value: Some(0),
            mem_addr: None,
        },
    ];

    let result = compare_traces_with_config(&traces, &traces, &CompareConfig::default());
    assert_eq!(result.matched, 2);
    assert!(result.divergence.is_none());
}

#[test]
fn test_compare_traces_missing_reg_strict() {
    let expected = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: Some(1),
        rd_value: Some(0),
        mem_addr: None,
    }];

    let actual = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: None, // Missing!
        rd_value: None,
        mem_addr: None,
    }];

    let config = CompareConfig {
        strict_reg_writes: true,
        ..Default::default()
    };
    let result = compare_traces_with_config(&expected, &actual, &config);
    assert!(result.divergence.is_some());
    assert_eq!(
        result.divergence.as_ref().unwrap().kind,
        DivergenceKind::MissingRegWrite
    );
}

#[test]
fn test_compare_traces_missing_reg_lenient() {
    let expected = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: Some(1),
        rd_value: Some(0),
        mem_addr: None,
    }];

    let actual = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: None,
        rd_value: None,
        mem_addr: None,
    }];

    let config = CompareConfig {
        strict_reg_writes: false,
        ..Default::default()
    };
    let result = compare_traces_with_config(&expected, &actual, &config);
    assert!(result.divergence.is_none());
    assert_eq!(result.matched, 1);
}

#[test]
fn test_compare_traces_diverge_value() {
    let expected = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: Some(1),
        rd_value: Some(0),
        mem_addr: None,
    }];

    let actual = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: Some(1),
        rd_value: Some(42), // Different!
        mem_addr: None,
    }];

    let result = compare_traces_with_config(&expected, &actual, &CompareConfig::default());
    assert!(result.divergence.is_some());
    assert_eq!(
        result.divergence.as_ref().unwrap().kind,
        DivergenceKind::RegValue
    );
}

#[test]
fn test_align_traces_at_entry() {
    let spike = vec![
        TraceEntry {
            pc: 0x1000,
            opcode: 0x1,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
        TraceEntry {
            pc: 0x1004,
            opcode: 0x2,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
        TraceEntry {
            pc: 0x8000_0000,
            opcode: 0x3,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
    ];

    let rvr = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x3,
        rd: None,
        rd_value: None,
        mem_addr: None,
    }];

    let (aligned_spike, aligned_rvr) = align_traces_at(&spike, &rvr, 0x8000_0000);
    assert_eq!(aligned_spike.len(), 1);
    assert_eq!(aligned_rvr.len(), 1);
    assert_eq!(aligned_spike[0].pc, 0x8000_0000);
}

#[test]
fn test_compare_traces_expected_tail() {
    let expected = vec![
        TraceEntry {
            pc: 0x8000_0000,
            opcode: 0x0000_0093,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
        TraceEntry {
            pc: 0x8000_0004,
            opcode: 0x0000_0013,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
    ];
    let actual = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: None,
        rd_value: None,
        mem_addr: None,
    }];

    let result = compare_traces_with_config(&expected, &actual, &CompareConfig::default());
    assert!(result.divergence.is_some());
    assert_eq!(
        result.divergence.as_ref().unwrap().kind,
        DivergenceKind::ExpectedTail
    );
}

#[test]
fn test_compare_traces_actual_tail() {
    let expected = vec![TraceEntry {
        pc: 0x8000_0000,
        opcode: 0x0000_0093,
        rd: None,
        rd_value: None,
        mem_addr: None,
    }];
    let actual = vec![
        TraceEntry {
            pc: 0x8000_0000,
            opcode: 0x0000_0093,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
        TraceEntry {
            pc: 0x8000_0004,
            opcode: 0x0000_0013,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
    ];

    let result = compare_traces_with_config(&expected, &actual, &CompareConfig::default());
    assert!(result.divergence.is_some());
    assert_eq!(
        result.divergence.as_ref().unwrap().kind,
        DivergenceKind::ActualTail
    );
}

#[test]
fn test_compare_traces_stop_on_first_false_records_first_divergence() {
    let expected = vec![
        TraceEntry {
            pc: 0x8000_0000,
            opcode: 0x0000_0093,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
        TraceEntry {
            pc: 0x8000_0004,
            opcode: 0x0000_0013,
            rd: Some(1),
            rd_value: Some(1),
            mem_addr: None,
        },
    ];

    let actual = vec![
        TraceEntry {
            pc: 0x8000_0000,
            opcode: 0x0000_0093,
            rd: None,
            rd_value: None,
            mem_addr: None,
        },
        TraceEntry {
            pc: 0x8000_0004,
            opcode: 0x0000_0013,
            rd: Some(1),
            rd_value: Some(2),
            mem_addr: None,
        },
    ];

    let config = CompareConfig {
        stop_on_first: false,
        ..Default::default()
    };
    let result = compare_traces_with_config(&expected, &actual, &config);
    assert!(result.divergence.is_some());
    assert_eq!(
        result.divergence.as_ref().unwrap().kind,
        DivergenceKind::RegValue
    );
}
