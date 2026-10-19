struct SocketWitnessTraceScope {
    enabled: bool,
    events: Vec<SocketRecoveryTraceEvent>,
}

impl SocketWitnessTraceScope {
    fn enter(trace: bool) -> Self {
        let enabled = SOCKET_TRACE_ENABLED.with(|value| value.get());
        let events = take_socket_recovery_trace_events();
        set_socket_recovery_trace_enabled(trace);
        Self { enabled, events }
    }
}

impl Drop for SocketWitnessTraceScope {
    fn drop(&mut self) {
        set_socket_recovery_trace_enabled(self.enabled);
        SOCKET_TRACE_EVENTS.with(|events| {
            *events.borrow_mut() = std::mem::take(&mut self.events);
        });
    }
}

#[hypothesis_test]
fn refactor_socket_source_boundary_and_recovery() {
    let tree = HuffmanTree::new();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = "amazon_authority_runeword.d2s";
    let bytes =
        std::fs::read(root.join("tests/fixtures/savegames/original").join(fixture)).unwrap();
    let _state = DepthWitnessState::enter(0, false);
    let stream = crate::domain::item::stream::PlayerItemStream::read(&bytes, &tree).unwrap();
    assert_eq!(
        stream.dialect,
        crate::domain::item::stream::ItemStreamDialect::Alpha
    );
    assert_eq!(stream.items.len(), 5);
    let host = &stream.items[4];
    assert_eq!(host.code, "xrs ");
    assert_eq!((host.range.start, host.range.end), (7562, 8306));
    assert_eq!(host.socketed_items.len(), 3);
    let expected = [
        ("r15", 8000, 8088),
        ("r13", 8088, 8176),
        ("r08", 8176, 8264),
    ];
    let mut rows = Vec::new();
    for (child, (expected_code, target, end)) in host.socketed_items.iter().zip(expected) {
        assert_eq!(child.code, expected_code);
        assert_eq!((child.range.start, child.range.end), (target, end));
        for distance in [0u64, 1, 64, 65] {
            for marker in [false, true] {
                for code in ["plain", "xrs"] {
                    for trace in [false, true] {
                        let _state = DepthWitnessState::enter(0, false);
                        let _trace = SocketWitnessTraceScope::enter(trace);
                        let position = target - distance;
                        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
                        reader.skip(position as u32).unwrap();
                        let mut cursor = BitCursor::new(reader);
                        cursor.set_pos(position);
                        cursor.set_limit(position + 9);
                        cursor.set_trace(trace);
                        cursor.push_context("source-owner");
                        let before = limit_probe_snapshot(&cursor);
                        let offsets = [target];
                        let codes =
                            std::collections::HashMap::from([(target, expected_code.to_string())]);
                        let mut axiom = StatsAxiom::new(5, ItemQuality::Normal, true);
                        axiom.code = code.into();
                        axiom.is_socketed = true;
                        let mut calls = Vec::new();
                        let result = read_property_list(
                            &mut cursor,
                            code,
                            5,
                            PropertyReaderContext {
                                bytes: &bytes,
                                item_start_bit: 0,
                            },
                            &tree,
                            true,
                            false,
                            &axiom,
                            if marker { Some(&offsets) } else { None },
                            if marker { Some(&codes) } else { None },
                            |data, child_position, _, index, alpha| {
                                depth_witness_state(1, true);
                                assert_eq!(data, bytes.as_slice());
                                calls.push(serde_json::json!({"position": child_position, "index": index, "alpha": alpha}));
                                let mut parsed = crate::domain::item::Item::default();
                                parsed.code = "r09".into();
                                parsed.body.code = "r09".into();
                                Ok((parsed, 3))
                            },
                        );
                        depth_witness_state(0, false);
                        assert!(cursor.recorded_bits().iter().all(|bit| bit.offset
                            < (bytes.len() * 8) as u64
                            && bit.bit
                                == (bytes[bit.offset as usize / 8] & (1 << (bit.offset % 8))
                                    != 0)));
                        let outcome = match result {
                            Ok((properties, complete, term, children)) => serde_json::json!({
                                "kind": "ok", "properties": properties, "complete": complete, "term": term,
                                "children": children.iter().map(budget_boundary_item).collect::<Vec<_>>()
                            }),
                            Err(failure) => {
                                serde_json::json!({"kind": "error", "failure": limit_probe_failure(&failure)})
                            }
                        };
                        let events: Vec<_> = take_socket_recovery_trace_events().into_iter().map(|event|
                            serde_json::json!({"stage": event.stage, "current": event.current_rel_pos,
                                "next": event.next_marker, "observed": event.observed_marker, "note": event.note})
                        ).collect();
                        if !trace {
                            assert!(events.is_empty());
                        }
                        assert_eq!(outcome["kind"], "ok");
                        assert_eq!(outcome["properties"], serde_json::json!([]));
                        assert_eq!(outcome["complete"], true);
                        assert_eq!(outcome["term"], false);
                        let has_child = marker && distance <= 1;
                        assert_eq!(calls.len(), usize::from(has_child));
                        assert_eq!(
                            outcome["children"].as_array().unwrap().len(),
                            usize::from(has_child)
                        );
                        if has_child {
                            assert_eq!(outcome["children"][0]["code"], expected_code);
                            assert_eq!(cursor.pos(), target + 3);
                        } else {
                            assert_eq!(cursor.pos(), position);
                        }
                        assert_eq!(cursor.limit(), Some(position + 9));
                        assert_eq!(cursor.context_stack(), vec!["source-owner"]);
                        let stops: Vec<_> = events
                            .iter()
                            .filter(|event| event["stage"] == "post_loop")
                            .map(|event| event["note"].as_str().unwrap())
                            .collect();
                        let expected_stop = if marker && matches!(distance, 1 | 64) {
                            Some("main_loop_break_on_child_marker")
                        } else if code == "plain" {
                            Some("main_loop_break_on_plausible_header")
                        } else {
                            None
                        };
                        assert_eq!(
                            stops,
                            if trace {
                                expected_stop.into_iter().collect::<Vec<_>>()
                            } else {
                                Vec::new()
                            }
                        );

                        rows.push(serde_json::json!({"source_code": expected_code, "target": target, "distance": distance,
                            "marker": marker, "host": code, "trace": trace, "before": before,
                            "after": limit_probe_snapshot(&cursor), "outcome": outcome, "calls": calls, "events": events}));
                    }
                }
            }
        }
    }
    assert_eq!(rows.len(), 96);
    let report = serde_json::json!({"schema": 1, "fixture": fixture, "source_host": budget_boundary_item(host), "case_count": rows.len(), "rows": rows});
    if let Ok(output) = std::env::var("D2R_REFACTOR_SOCKET_OUTPUT") {
        let path = std::path::PathBuf::from(output);
        assert!(path.is_absolute() && !path.exists());
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
