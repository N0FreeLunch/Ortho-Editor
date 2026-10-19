#[hypothesis_test]
fn refactor_alias_source_scanner_defaults_and_precedence() {
    let tree = HuffmanTree::new();
    let mut rows = Vec::new();
    let hints = [
        None,
        Some(""),
        Some("ww"),
        Some("gcw"),
        Some(" ww "),
        Some(" gcw "),
        Some("r13"),
        Some("jew"),
        Some("WW"),
        Some("mystery"),
    ];
    for hint in hints {
        for parsed_code in ["ww", "gcw", " r08 ", "mystery"] {
            for failed in [false, true] {
                for trace in [false, true] {
                    let _state = DepthWitnessState::enter(0, false);
                    let _trace = SocketWitnessTraceScope::enter(trace);
                    let bits: Vec<_> = (0..128).map(|index| index % 3 == 1).collect();
                    let bytes = depth_witness_packed(&bits);
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    cursor.set_limit(128);
                    cursor.set_trace(trace);
                    cursor.push_context("alias-owner");
                    let before = limit_probe_snapshot(&cursor);
                    let offsets = [8];
                    let codes =
                        hint.map(|code| std::collections::HashMap::from([(8, code.to_string())]));
                    let mut calls = 0;
                    let result = limit_probe_read(
                        &mut cursor,
                        &tree,
                        "plain",
                        true,
                        true,
                        true,
                        Some(&offsets),
                        codes.as_ref(),
                        |data, position, _, index, alpha| {
                            calls += 1;
                            depth_witness_state(1, true);
                            assert!(data.is_empty() && alpha);
                            assert_eq!((position, index), (8, 0));
                            if failed {
                                return Err(depth_witness_failure());
                            }
                            let mut child = crate::domain::item::Item::default();
                            child.code = parsed_code.into();
                            child.body.code = "body-sentinel".into();
                            child.header.id = Some(77);
                            Ok((child, 5))
                        },
                    )
                    .unwrap();
                    depth_witness_state(0, false);
                    assert_eq!(calls, 1);
                    assert_eq!(result["properties"], serde_json::json!([]));
                    assert_eq!(result["complete"], true);
                    assert_eq!(result["term"], false);
                    let child_expected =
                        !failed || hint.map(|value| !value.trim().is_empty()).unwrap_or(false);
                    assert_eq!(
                        result["children"].as_array().unwrap().len(),
                        usize::from(child_expected)
                    );
                    assert_eq!(
                        cursor.pos(),
                        if !child_expected {
                            8
                        } else if failed {
                            80
                        } else {
                            13
                        }
                    );
                    assert_eq!(cursor.limit(), Some(128));
                    assert_eq!(cursor.context_stack(), vec!["alias-owner"]);
                    assert_eq!(
                        cursor
                            .recorded_bits()
                            .iter()
                            .map(|bit| bit.bit)
                            .collect::<Vec<_>>(),
                        bits[..cursor.pos() as usize]
                    );
                    if child_expected {
                        let child = &result["children"][0];
                        assert_eq!(child["code"], child["body_code"]);
                        assert_eq!(child["mode"], 6);
                        assert_eq!(child["header_mode"], 6);
                        assert_eq!(
                            child["id"],
                            if failed {
                                serde_json::Value::Null
                            } else {
                                serde_json::json!(77)
                            }
                        );
                    }
                    let events: Vec<_> = take_socket_recovery_trace_events().into_iter().map(|event|
                        serde_json::json!({"stage":event.stage,"current":event.current_rel_pos,"next":event.next_marker,"observed":event.observed_marker,"note":event.note})).collect();
                    if !trace {
                        assert!(events.is_empty());
                    }
                    rows.push(serde_json::json!({"hint":hint,"parsed_code":parsed_code,"failed":failed,"trace":trace,"before":before,"after":limit_probe_snapshot(&cursor),"calls":calls,"result":result,"events":events}));
                }
            }
        }
    }
    assert_eq!(rows.len(), 160);
    if let Ok(output) = std::env::var("D2R_REFACTOR_ALIAS_OUTPUT") {
        let path = std::path::PathBuf::from(output);
        assert!(path.is_absolute() && !path.exists());
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"schema":1,"case_count":160,"rows":rows}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
