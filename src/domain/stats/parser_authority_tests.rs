#[hypothesis_test]
fn refactor_authority_phase_sources_and_admission() {
    let tree = HuffmanTree::new();
    let codes = ["xrs", "c8xr", "rhd", "wa2", " wa2 ", "plain", "ww", ""];
    let mut lists = Vec::new();
    let mut terminals = Vec::new();
    let mut roots = Vec::new();
    for code in codes {
        for version in [0, 3, 5, 8] {
            for saved in [false, true] {
                for compact in [false, true] {
                    let _state = DepthWitnessState::enter(0, false);
                    let bytes: &[u8] = &[];
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(bytes), LittleEndian));
                    cursor.set_trace(true);
                    cursor.push_context("phase-owner");
                    let before = limit_probe_snapshot(&cursor);
                    let result = read_item_stats(
                        &mut cursor,
                        code,
                        version,
                        None,
                        &tree,
                        saved,
                        Some(ItemQuality::Normal),
                        0,
                        false,
                        false,
                        false,
                        compact,
                        false,
                    );
                    let outcome = match result {
                        Ok((properties, complete, term, extra, shadow, padding, children)) => {
                            serde_json::json!({
                                "kind":"ok","properties":properties,"complete":complete,"term":term,"extra":extra,"shadow":shadow,"padding":padding,
                                "children":children.iter().map(budget_boundary_item).collect::<Vec<_>>()
                            })
                        }
                        Err(failure) => {
                            serde_json::json!({"kind":"error","failure":limit_probe_failure(&failure)})
                        }
                    };
                    depth_witness_state(0, false);
                    assert_eq!(cursor.context_stack(), vec!["phase-owner"]);
                    roots.push(serde_json::json!({"code":code,"version":version,"saved":saved,"compact":compact,"before":before,"after":limit_probe_snapshot(&cursor),"outcome":outcome}));

                    for runeword in [false, true] {
                        let _state = DepthWitnessState::enter(0, false);
                        let mut bits = vec![true; 9];
                        bits.extend([false; 7]);
                        let bytes = depth_witness_packed(&bits);
                        let mut cursor =
                            BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                        cursor.set_limit(9);
                        cursor.set_trace(true);
                        cursor.push_context("phase-owner");
                        let mut axiom = StatsAxiom::new(version, ItemQuality::Normal, saved);
                        axiom.code = code.into();
                        axiom.is_compact = compact;
                        let before = limit_probe_snapshot(&cursor);
                        let rhythm = axiom.property_rhythm(runeword, false, compact, 511);
                        let result = parse_single_property(
                            &mut cursor,
                            version,
                            &tree,
                            runeword,
                            &axiom,
                            PropertyReaderContext {
                                bytes: &bytes,
                                item_start_bit: 0,
                            },
                            |_, _, _, _, _| panic!("terminator cannot recover child"),
                        );
                        let outcome = match result {
                            Ok(Some((property, complete, term, children))) => {
                                serde_json::json!({"kind":"property","property":property,"complete":complete,"term":term,"children":children.len()})
                            }
                            Ok(None) => serde_json::json!({"kind":"none"}),
                            Err(failure) => {
                                serde_json::json!({"kind":"error","failure":limit_probe_failure(&failure)})
                            }
                        };
                        depth_witness_state(0, false);
                        assert_eq!(cursor.context_stack(), vec!["phase-owner"]);
                        assert_eq!(cursor.pos(), 9);
                        assert_eq!(
                            cursor
                                .recorded_bits()
                                .iter()
                                .map(|bit| bit.bit)
                                .collect::<Vec<_>>(),
                            vec![true; 9]
                        );
                        terminals.push(serde_json::json!({"code":code,"version":version,"saved":saved,"compact":compact,"runeword":runeword,
                            "admitted":axiom.is_alpha(),"rhythm":{"id_bits":rhythm.id_bits,"value_bits":rhythm.value_bits,"terminal":rhythm.has_terminal_bit,"extra":rhythm.has_extra_terminal_bit},
                            "before":before,"after":limit_probe_snapshot(&cursor),"outcome":outcome}));

                        for axiom_code in ["plain", "c8xr"] {
                            for socketed in [false, true] {
                                let _state = DepthWitnessState::enter(11, false);
                                let mut cursor = BitCursor::new(BitReader::endian(
                                    Cursor::new(&[] as &[u8]),
                                    LittleEndian,
                                ));
                                cursor.set_trace(true);
                                cursor.push_context("phase-owner");
                                let mut axiom =
                                    StatsAxiom::new(version, ItemQuality::Normal, saved);
                                axiom.code = axiom_code.into();
                                axiom.is_compact = compact;
                                axiom.is_socketed = socketed;
                                let before = limit_probe_snapshot(&cursor);
                                let (properties, complete, term, children) = read_property_list(
                                    &mut cursor,
                                    code,
                                    version,
                                    PropertyReaderContext {
                                        bytes: &[],
                                        item_start_bit: 0,
                                    },
                                    &tree,
                                    runeword,
                                    false,
                                    &axiom,
                                    None,
                                    None,
                                    |_, _, _, _, _| {
                                        panic!("depth-rejected list cannot recover child")
                                    },
                                )
                                .unwrap();
                                depth_witness_state(11, false);
                                assert!(
                                    properties.is_empty()
                                        && children.is_empty()
                                        && !complete
                                        && !term
                                );
                                assert_eq!(cursor.pos(), 0);
                                assert_eq!(cursor.context_stack(), vec!["phase-owner"]);
                                lists.push(serde_json::json!({"code":code,"axiom_code":axiom_code,"version":version,"saved":saved,"compact":compact,"socketed":socketed,"runeword":runeword,
                                    "admitted":axiom.is_alpha(),"before":before,"after":limit_probe_snapshot(&cursor)}));
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        (roots.len(), lists.len(), terminals.len()),
        (128, 1024, 256)
    );
    let raw_term = terminals
        .iter()
        .find(|row| {
            row["code"] == "xrs"
                && row["version"] == 3
                && row["saved"] == true
                && row["compact"] == false
                && row["runeword"] == false
        })
        .unwrap();
    let plain_term = terminals
        .iter()
        .find(|row| {
            row["code"] == "plain"
                && row["version"] == 3
                && row["saved"] == true
                && row["compact"] == false
                && row["runeword"] == false
        })
        .unwrap();
    assert_eq!(raw_term["admitted"], false);
    assert_eq!(raw_term["outcome"]["kind"], "property");
    assert_eq!(plain_term["outcome"]["kind"], "error");
    let external = lists
        .iter()
        .find(|row| {
            row["code"] == "xrs"
                && row["axiom_code"] == "plain"
                && row["version"] == 0
                && row["saved"] == true
                && row["compact"] == false
                && row["socketed"] == true
                && row["runeword"] == false
        })
        .unwrap();
    let stored = lists
        .iter()
        .find(|row| {
            row["code"] == "plain"
                && row["axiom_code"] == "c8xr"
                && row["version"] == 0
                && row["saved"] == true
                && row["compact"] == false
                && row["socketed"] == true
                && row["runeword"] == false
        })
        .unwrap();
    assert_eq!(external["after"]["limit"].as_u64(), Some(u64::MAX));
    assert!(stored["after"]["limit"].is_null());
    let unadmitted = lists
        .iter()
        .find(|row| {
            row["code"] == "xrs"
                && row["axiom_code"] == "c8xr"
                && row["version"] == 3
                && row["saved"] == true
                && row["compact"] == false
                && row["socketed"] == true
                && row["runeword"] == false
        })
        .unwrap();
    assert!(unadmitted["after"]["limit"].is_null());
    if let Ok(output) = std::env::var("D2R_REFACTOR_AUTHORITY_OUTPUT") {
        let path = std::path::PathBuf::from(output);
        assert!(path.is_absolute() && !path.exists());
        std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"case_count":1408,"roots":roots,"lists":lists,"terminals":terminals})).unwrap()).unwrap();
    }
}
