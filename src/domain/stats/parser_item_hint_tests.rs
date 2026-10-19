#[d2r_macros::radar_test]
fn refactor_item_hint_context_and_cursor_target_contract() {
    use crate::domain::item::Item;
    let tree = HuffmanTree::new();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let bytes =
        std::fs::read(root.join("tests/fixtures/savegames/original/amazon_authority_runeword.d2s"))
            .unwrap();
    let _state = DepthWitnessState::enter(0, false);
    let producer = crate::domain::item::stream::PlayerItemStream::read(&bytes, &tree).unwrap();
    assert_eq!(
        producer.dialect,
        crate::domain::item::stream::ItemStreamDialect::Alpha
    );
    assert_eq!(producer.items.len(), 5);
    let source = &producer.items[4];
    assert_eq!(source.code, "xrs ");
    assert_eq!((source.range.start, source.range.end), (7562, 8306));
    let hints = [
        None,
        Some("xrs "),
        Some("c8xr"),
        Some("rhd"),
        Some("wa2"),
        Some("ww"),
        Some("gcw"),
        Some("plain"),
        Some(" xrs "),
    ];
    let mut cases = Vec::new();
    for hint in hints {
        for context in [false, true] {
            for alpha in [false, true] {
                for compact in [false, true] {
                    cases.push(("main", hint, context, alpha, compact, "interval", 0u64));
                }
            }
            for limit in ["none", "max"] {
                cases.push(("continuation", hint, context, true, false, limit, 0));
            }
            cases.push(("base-control", hint, context, true, false, "interval", 4096));
        }
    }
    assert_eq!(cases.len(), 126);
    let mut rows = Vec::new();
    let mut qualified_controls = 0;
    for (group, hint, context, alpha, compact, limit_kind, base) in cases {
        let _state = DepthWitnessState::enter(0, false);
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        reader.skip(source.range.start as u32).unwrap();
        let mut cursor = BitCursor::new(reader);
        cursor.set_pos(base + source.range.start);
        cursor.base_pos = base;
        cursor.set_trace(true);
        let initial = match limit_kind {
            "interval" => Some(base + source.range.end),
            "max" => Some(u64::MAX),
            _ => None,
        };
        if let Some(limit) = initial {
            cursor.set_limit(limit);
        }
        let before = limit_probe_snapshot(&cursor);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Item::from_reader_with_context(
                &mut cursor,
                &tree,
                if context {
                    Some((&bytes, source.range.start))
                } else {
                    None
                },
                alpha,
                4,
                Some(compact),
                hint,
            )
        }));
        let (kind, model, raw_match) = match result {
            Ok(Ok(item)) => {
                let raw_match = item.bits.iter().all(|bit| {
                    bit.offset >= base
                        && bit.offset - base < (bytes.len() * 8) as u64
                        && bit.bit
                            == (bytes[(bit.offset - base) as usize / 8]
                                & (1 << ((bit.offset - base) % 8))
                                != 0)
                });
                ("ok", budget_boundary_item(&item), Some(raw_match))
            }
            Ok(Err(failure)) => ("error", limit_probe_failure(&failure), None),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|value| (*value).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic payload".into());
                ("panic", serde_json::json!({"panic":message}), None)
            }
        };
        if base == 0 && hint == Some("xrs ") && context && alpha && !compact {
            assert_eq!(kind, "ok", "declared real source control must succeed");
            assert_eq!(raw_match, Some(true));
            assert_eq!(model["total_bits"], 862);
            assert_eq!(cursor.pos(), 8424);
            qualified_controls += 1;
        }
        rows.push(serde_json::json!({"group":group,"hint":hint,"context":context,"alpha":alpha,"compact":compact,"limit_kind":limit_kind,"base":base,
            "before":before,"after":limit_probe_snapshot(&cursor),"kind":kind,"model":model,"raw_match":raw_match}));
    }
    assert_eq!(qualified_controls, 3);
    if let Ok(output) = std::env::var("D2R_REFACTOR_ITEM_HINT_OUTPUT") {
        let path = std::path::PathBuf::from(output);
        assert!(path.is_absolute() && !path.exists());
        std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"case_count":126,"qualified_controls":qualified_controls,
            "source_host":budget_boundary_item(source),"rows":rows,"limit":"Override/base controls are negative diagnostics, not validated gameplay entities or fidelity scores."})).unwrap()).unwrap();
    }
}
