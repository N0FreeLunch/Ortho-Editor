fn limit_probe_snapshot<R: BitRead>(cursor: &BitCursor<R>) -> serde_json::Value {
    let mut value = stats_scope_snapshot(cursor);
    value["remaining"] = serde_json::json!(cursor.remaining());
    value
}

fn limit_probe_failure(failure: &crate::error::ParsingFailure) -> serde_json::Value {
    serde_json::json!({"error": failure.error.to_string(), "contexts": failure.context_stack,
            "bit_offset": failure.bit_offset, "relative_offset": failure.context_relative_offset,
            "hint": failure.hint})
}

fn limit_probe_read<R: BitRead, F>(
    cursor: &mut BitCursor<R>,
    tree: &HuffmanTree,
    code: &str,
    alpha: bool,
    runeword: bool,
    socketed: bool,
    offsets: Option<&[u64]>,
    codes: Option<&std::collections::HashMap<u64, String>>,
    callback: F,
) -> ParsingResult<serde_json::Value>
where
    F: FnMut(
        &[u8],
        u64,
        &HuffmanTree,
        usize,
        bool,
    ) -> ParsingResult<(crate::domain::item::Item, u64)>,
{
    let mut axiom = StatsAxiom::new(5, ItemQuality::Normal, alpha);
    axiom.code = code.into();
    axiom.is_socketed = socketed;
    read_property_list(
        cursor,
        code,
        5,
        PropertyReaderContext {
            bytes: &[],
            item_start_bit: 0,
        },
        tree,
        runeword,
        false,
        &axiom,
        offsets,
        codes,
        callback,
    )
    .map(|(properties, complete, term, children)| {
        serde_json::json!({
            "properties": properties, "complete": complete, "term": term,
            "children": children.iter().map(|child| serde_json::json!({"code": child.code,
                "body_code": child.body.code, "mode": child.mode, "header_mode": child.header.mode,
                "id": child.header.id})).collect::<Vec<_>>()
        })
    })
}

fn limit_probe_selectors() -> [(&'static str, bool, bool, bool); 12] {
    [
        ("plain", true, false, false),
        ("plain", true, false, true),
        ("plain", true, true, false),
        ("plain", true, true, true),
        ("wa2", true, false, true),
        (" wa2 ", true, false, true),
        ("xrs", true, false, true),
        ("c8xr", true, false, true),
        ("rhd", true, false, true),
        ("\u{de}.", true, false, true),
        ("ucb8", true, false, true),
        ("plain", false, true, true),
    ]
}

fn limit_probe_actual_alpha(code: &str, alpha: bool, runeword: bool, socketed: bool) -> bool {
    alpha
        && socketed
        && (runeword || matches!(code.trim(), "xrs" | "c8xr" | "rhd" | "wa2" | "\u{de}."))
}

fn limit_probe_physical_bits(bytes: &[u8]) -> Vec<bool> {
    (0..bytes.len() * 8)
        .map(|i| bytes[i / 8] & (1 << (i % 8)) != 0)
        .collect()
}

#[hypothesis_test]
fn refactor_limit_probe_normal_selectors_and_committed_horizon() {
    let tree = HuffmanTree::new();
    let mut rows = Vec::new();
    for (selector, (code, alpha, runeword, socketed)) in
        limit_probe_selectors().into_iter().enumerate()
    {
        for initial in [None, Some(64u64), Some(4096), Some(u64::MAX)] {
            for offset in [0usize, 3] {
                for trace in [false, true] {
                    let _state = DepthWitnessState::enter(0, false);
                    let mut bits = depth_witness_prefix(offset);
                    bits.extend([true; 9]);
                    if !alpha {
                        bits.push(true);
                        while bits.len() % 8 != 0 {
                            bits.push(false);
                        }
                    }
                    let end = bits.len();
                    bits.extend([false, true, true, false, true]);
                    let bytes = depth_witness_packed(&bits);
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    for bit in depth_witness_prefix(offset) {
                        assert_eq!(cursor.read_bit().unwrap(), bit);
                    }
                    if let Some(limit) = initial {
                        cursor.set_limit(limit);
                    }
                    cursor.set_trace(trace);
                    cursor.push_context("owner");
                    let before = limit_probe_snapshot(&cursor);
                    let result = limit_probe_read(
                        &mut cursor,
                        &tree,
                        code,
                        alpha,
                        runeword,
                        socketed,
                        None,
                        None,
                        |_, _, _, _, _| panic!("terminal-only list must not recover a child"),
                    )
                    .unwrap();
                    let actual = limit_probe_actual_alpha(code, alpha, runeword, socketed);
                    let horizon = end as u64 + 512;
                    let expected = if actual {
                        Some(initial.map_or(horizon, |limit| limit.min(horizon)))
                    } else {
                        initial
                    };
                    assert_eq!(cursor.limit(), expected);
                    assert_eq!(cursor.pos(), end as u64);
                    assert_eq!(result["properties"].as_array().unwrap().len(), 1);
                    assert_eq!(result["properties"][0]["stat_id"], 511);
                    assert_eq!(result["complete"], true);
                    assert_eq!(result["term"], !alpha);
                    assert_eq!(result["children"], serde_json::json!([]));
                    assert_eq!(cursor.context_stack(), vec!["owner"]);
                    assert_eq!(
                        cursor
                            .recorded_bits()
                            .iter()
                            .map(|b| b.bit)
                            .collect::<Vec<_>>(),
                        bits[..end]
                    );
                    depth_witness_state(0, false);
                    let after = limit_probe_snapshot(&cursor);
                    for &bit in &bits[end..] {
                        assert_eq!(cursor.read_bit().unwrap(), bit);
                    }
                    rows.push(serde_json::json!({"selector": selector, "code": code, "alpha": alpha, "runeword": runeword, "socketed": socketed, "actual_alpha": actual, "initial": initial, "offset": offset, "trace": trace, "before": before, "after": after, "result": result, "exit_class": "normal_finalization", "unread_tail_preserved": true}));
                }
            }
        }
    }
    lifetime_probe_report("limit-normal", rows, 192);
}

#[hypothesis_test]
fn refactor_limit_probe_rejected_depth_retains_setup_state() {
    let tree = HuffmanTree::new();
    let mut rows = Vec::new();
    for (selector, (code, alpha, runeword, socketed)) in
        limit_probe_selectors().into_iter().enumerate()
    {
        for initial in [None, Some(64u64), Some(u64::MAX)] {
            for depth in [11, u32::MAX] {
                for offset in [0usize, 3] {
                    for trace in [false, true] {
                        let _state = DepthWitnessState::enter(depth, false);
                        let mut bits = depth_witness_prefix(offset);
                        bits.extend([true; 16]);
                        let bytes = depth_witness_packed(&bits);
                        let mut cursor =
                            BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                        for bit in depth_witness_prefix(offset) {
                            assert_eq!(cursor.read_bit().unwrap(), bit);
                        }
                        if let Some(limit) = initial {
                            cursor.set_limit(limit);
                        }
                        cursor.set_trace(trace);
                        cursor.push_context("owner");
                        let before = limit_probe_snapshot(&cursor);
                        let result = limit_probe_read(
                            &mut cursor,
                            &tree,
                            code,
                            alpha,
                            runeword,
                            socketed,
                            None,
                            None,
                            |_, _, _, _, _| panic!("rejected depth must not recover a child"),
                        )
                        .unwrap();
                        let actual = limit_probe_actual_alpha(code, alpha, runeword, socketed);
                        assert_eq!(
                            cursor.limit(),
                            if actual && initial.is_none() {
                                Some(u64::MAX)
                            } else {
                                initial
                            }
                        );
                        assert_eq!(cursor.pos(), offset as u64);
                        assert_eq!(
                            cursor
                                .recorded_bits()
                                .iter()
                                .map(|b| b.bit)
                                .collect::<Vec<_>>(),
                            depth_witness_prefix(offset)
                        );
                        assert_eq!(cursor.context_stack(), vec!["owner"]);
                        assert_eq!(
                            result,
                            serde_json::json!({"properties": [], "complete": false, "term": false, "children": []})
                        );
                        depth_witness_state(depth, false);
                        rows.push(serde_json::json!({"selector": selector, "code": code, "alpha": alpha, "runeword": runeword, "socketed": socketed, "actual_alpha": actual, "initial": initial, "depth": depth, "offset": offset, "trace": trace, "before": before, "after": limit_probe_snapshot(&cursor), "result": result, "exit_class": "depth_rejection"}));
                    }
                }
            }
        }
    }
    lifetime_probe_report("limit-depth", rows, 288);
}

struct LimitProbeRead<'a> {
    bytes: &'a [u8],
    position: usize,
    mode: &'static str,
}

impl std::io::Read for LimitProbeRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.position < self.bytes.len() {
            let count = buffer.len().min(self.bytes.len() - self.position);
            buffer[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
            self.position += count;
            return Ok(count);
        }
        match self.mode {
            "eof" => Ok(0),
            "error" => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "limit probe io sentinel",
            )),
            "panic" => std::panic::panic_any("limit probe io panic"),
            _ => panic!("unknown limit probe IO mode"),
        }
    }
}

#[hypothesis_test]
fn refactor_limit_probe_first_io_exit_classes() {
    let tree = HuffmanTree::new();
    let mut rows = Vec::new();
    for active in [false, true] {
        for initial in [None, Some(4096u64), Some(u64::MAX)] {
            for offset in [0usize, 3] {
                for trace in [false, true] {
                    for mode in ["eof", "error", "panic"] {
                        let _state = DepthWitnessState::enter(0, false);
                        let bytes = depth_witness_packed(&depth_witness_prefix(offset));
                        let physical = limit_probe_physical_bits(&bytes);
                        let reader = LimitProbeRead {
                            bytes: &bytes,
                            position: 0,
                            mode,
                        };
                        let mut cursor = BitCursor::new(BitReader::endian(reader, LittleEndian));
                        for bit in depth_witness_prefix(offset) {
                            assert_eq!(cursor.read_bit().unwrap(), bit);
                        }
                        if let Some(limit) = initial {
                            cursor.set_limit(limit);
                        }
                        cursor.set_trace(trace);
                        cursor.push_context("owner");
                        let before = limit_probe_snapshot(&cursor);
                        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            limit_probe_read(
                                &mut cursor,
                                &tree,
                                "plain",
                                true,
                                active,
                                true,
                                None,
                                None,
                                |_, _, _, _, _| {
                                    panic!("partial first property must not recover a child")
                                },
                            )
                        }));
                        let (class, result) = match caught {
                            Err(payload) => {
                                assert_eq!(mode, "panic");
                                assert_eq!(
                                    payload.downcast_ref::<&str>(),
                                    Some(&"limit probe io panic")
                                );
                                (
                                    "io_unwind",
                                    serde_json::json!({"panic_payload": "limit probe io panic"}),
                                )
                            }
                            Ok(Ok(value)) => {
                                assert_eq!(mode, "eof");
                                assert_eq!(
                                    value,
                                    serde_json::json!({"properties": [], "complete": true, "term": false, "children": []})
                                );
                                ("handled_eof_ok", value)
                            }
                            Ok(Err(failure)) => {
                                assert_eq!(mode, "error");
                                assert!(
                                    matches!(&failure.error, crate::error::ParsingError::Io(message) if message.contains("limit probe io sentinel"))
                                );
                                assert_eq!(failure.context_stack, vec!["owner"]);
                                assert_eq!(
                                    (failure.bit_offset, failure.context_relative_offset),
                                    (physical.len() as u64, (physical.len() - offset) as u64)
                                );
                                assert_eq!(
                                    failure.hint.as_deref(),
                                    Some("Possible end of bitstream reached unexpectedly.")
                                );
                                ("property_error", limit_probe_failure(&failure))
                            }
                        };
                        assert_eq!(
                            cursor.limit(),
                            if active && initial.is_none() {
                                Some(u64::MAX)
                            } else {
                                initial
                            }
                        );
                        assert_eq!(cursor.pos(), physical.len() as u64);
                        assert_eq!(
                            cursor
                                .recorded_bits()
                                .iter()
                                .map(|b| b.bit)
                                .collect::<Vec<_>>(),
                            physical
                        );
                        assert_eq!(cursor.context_stack(), vec!["owner"]);
                        depth_witness_state(0, false);
                        rows.push(serde_json::json!({"active": active, "initial": initial, "offset": offset, "trace": trace, "mode": mode, "exit_class": class, "before": before, "after": limit_probe_snapshot(&cursor), "result": result}));
                    }
                }
            }
        }
    }
    lifetime_probe_report("limit-io", rows, 72);
}

#[hypothesis_test]
fn refactor_limit_probe_scanner_exit_and_budget_state() {
    let tree = HuffmanTree::new();
    let mut rows = Vec::new();
    for route in [
        "success",
        "callback_error",
        "callback_panic",
        "skip_error",
        "marker_skip_error",
    ] {
        for initial in [None, Some(256u64), Some(u64::MAX)] {
            for offset in [0usize, 3] {
                for trace in [false, true] {
                    for depth in [0, 10] {
                        for nested in [false, true] {
                            let _state = DepthWitnessState::enter(depth, nested);
                            let short = matches!(route, "skip_error" | "marker_skip_error");
                            let mut bits = depth_witness_prefix(offset);
                            bits.extend((0..if short { 8 } else { 96 }).map(|i| i % 3 == 1));
                            let bytes = depth_witness_packed(&bits);
                            let physical = limit_probe_physical_bits(&bytes);
                            let mut cursor = BitCursor::new(BitReader::endian(
                                Cursor::new(&bytes),
                                LittleEndian,
                            ));
                            for bit in depth_witness_prefix(offset) {
                                assert_eq!(cursor.read_bit().unwrap(), bit);
                            }
                            if let Some(limit) = initial {
                                cursor.set_limit(limit);
                            }
                            cursor.set_trace(trace);
                            cursor.push_context("owner");
                            let before = limit_probe_snapshot(&cursor);
                            let target =
                                offset as u64 + if route == "marker_skip_error" { 24 } else { 8 };
                            let offsets = [target];
                            let codes =
                                std::collections::HashMap::from([(target, "r01".to_string())]);
                            let mut calls = 0;
                            let caught =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    limit_probe_read(
                                        &mut cursor,
                                        &tree,
                                        "plain",
                                        true,
                                        true,
                                        true,
                                        Some(&offsets),
                                        Some(&codes),
                                        |data, position, _, index, alpha| {
                                            calls += 1;
                                            assert_ne!(
                                                route, "marker_skip_error",
                                                "marker skip must fail before callback"
                                            );
                                            depth_witness_state(depth + 1, true);
                                            assert!(data.is_empty() && alpha);
                                            assert_eq!((position, index), (target, 0));
                                            if route == "callback_panic" {
                                                std::panic::panic_any("limit probe recovery panic");
                                            }
                                            if route == "callback_error" {
                                                return Err(depth_witness_failure());
                                            }
                                            let mut child = crate::domain::item::Item::default();
                                            child.code = "r09".into();
                                            child.body.code = "r09".into();
                                            child.header.id = Some(77);
                                            Ok((child, if route == "skip_error" { 9 } else { 5 }))
                                        },
                                    )
                                }));
                            let finalized = matches!(route, "success" | "callback_error");
                            let expected_end = match route {
                                "success" => target + 5,
                                "callback_error" => target + 72,
                                "callback_panic" => target,
                                _ => physical.len() as u64,
                            };
                            let (class, result) = match caught {
                                Err(payload) => {
                                    assert_eq!(route, "callback_panic");
                                    assert_eq!(
                                        payload.downcast_ref::<&str>(),
                                        Some(&"limit probe recovery panic")
                                    );
                                    (
                                        "recovery_unwind",
                                        serde_json::json!({"panic_payload": "limit probe recovery panic"}),
                                    )
                                }
                                Ok(Ok(value)) => {
                                    assert!(finalized);
                                    assert_eq!(value["properties"], serde_json::json!([]));
                                    assert_eq!(value["complete"], true);
                                    assert_eq!(value["term"], false);
                                    assert_eq!(value["children"].as_array().unwrap().len(), 1);
                                    let child = &value["children"][0];
                                    assert_eq!(child["code"], "r01");
                                    assert_eq!(child["body_code"], "r01");
                                    assert_eq!(child["mode"], 6);
                                    assert_eq!(child["header_mode"], 6);
                                    assert_eq!(
                                        child["id"],
                                        if route == "success" {
                                            serde_json::json!(77)
                                        } else {
                                            serde_json::Value::Null
                                        }
                                    );
                                    ("normal_finalization", value)
                                }
                                Ok(Err(failure)) => {
                                    assert!(short);
                                    assert!(
                                        matches!(&failure.error, crate::error::ParsingError::Io(message) if message.contains("failed to fill whole buffer") || message.contains("unexpected end of file"))
                                    );
                                    assert_eq!(failure.context_stack, vec!["owner"]);
                                    assert_eq!(
                                        (failure.bit_offset, failure.context_relative_offset),
                                        (expected_end, expected_end - offset as u64)
                                    );
                                    assert_eq!(
                                        failure.hint.as_deref(),
                                        Some("Possible end of bitstream reached unexpectedly.")
                                    );
                                    ("skip_error", limit_probe_failure(&failure))
                                }
                            };
                            assert_eq!(calls, usize::from(route != "marker_skip_error"));
                            let expected_limit = if finalized {
                                let horizon = expected_end + 512;
                                Some(initial.map_or(horizon, |limit| limit.min(horizon)))
                            } else if initial.is_none() {
                                Some(u64::MAX)
                            } else {
                                initial
                            };
                            assert_eq!(cursor.limit(), expected_limit);
                            assert_eq!(cursor.pos(), expected_end);
                            assert_eq!(
                                cursor
                                    .recorded_bits()
                                    .iter()
                                    .map(|b| b.bit)
                                    .collect::<Vec<_>>(),
                                physical[..expected_end as usize]
                            );
                            assert_eq!(cursor.context_stack(), vec!["owner"]);
                            depth_witness_state(depth, nested);
                            rows.push(serde_json::json!({"route": route, "initial": initial, "offset": offset, "trace": trace, "depth": depth, "nested": nested, "calls": calls, "target": target, "exit_class": class, "before": before, "after": limit_probe_snapshot(&cursor), "result": result}));
                        }
                    }
                }
            }
        }
    }
    lifetime_probe_report("limit-scanner", rows, 240);
}

#[hypothesis_test]
fn refactor_limit_probe_stats_consumer_handoff() {
    let tree = HuffmanTree::new();
    let mut rows = Vec::new();
    for version in [1, 5] {
        for initial in [None, Some(64u64), Some(u64::MAX)] {
            for offset in [0usize, 3] {
                for trace in [false, true] {
                    for full in [false, true] {
                        for v3 in [false, true] {
                            let _state = DepthWitnessState::enter(0, false);
                            let mut bits = depth_witness_prefix(offset);
                            if full {
                                bits.extend([true; 9]);
                                bits.extend([true, false]);
                            }
                            let end = bits.len();
                            if full {
                                bits.extend([false, true, true, false, true]);
                            }
                            let bytes = depth_witness_packed(&bits);
                            let physical = limit_probe_physical_bits(&bytes);
                            let mut cursor = BitCursor::new(BitReader::endian(
                                Cursor::new(&bytes),
                                LittleEndian,
                            ));
                            for bit in depth_witness_prefix(offset) {
                                assert_eq!(cursor.read_bit().unwrap(), bit);
                            }
                            if let Some(limit) = initial {
                                cursor.set_limit(limit);
                            }
                            cursor.set_trace(trace);
                            cursor.push_context("owner");
                            let before = limit_probe_snapshot(&cursor);
                            let result = if v3 {
                                read_item_stats_d2r_v3(
                                    &mut cursor,
                                    "plain",
                                    version,
                                    None,
                                    &tree,
                                    true,
                                    Some(ItemQuality::Normal),
                                    16 | (1 << 26),
                                    true,
                                    false,
                                    false,
                                    false,
                                    true,
                                )
                            } else {
                                read_item_stats(
                                    &mut cursor,
                                    "plain",
                                    version,
                                    None,
                                    &tree,
                                    true,
                                    Some(ItemQuality::Normal),
                                    16 | (1 << 26),
                                    true,
                                    false,
                                    false,
                                    false,
                                    true,
                                )
                            };
                            let (class, value) = if full {
                                let (properties, complete, term, extra, payload, skip, children) =
                                    result.unwrap();
                                assert_eq!(properties.len(), 1);
                                assert_eq!(properties[0].stat_id, 511);
                                assert_eq!(
                                    (properties[0].range.start, properties[0].range.end),
                                    (offset as u64, offset as u64 + 9)
                                );
                                assert!(
                                    complete
                                        && !term
                                        && payload.is_none()
                                        && skip.is_none()
                                        && children.is_empty()
                                );
                                assert_eq!(extra, Some(1));
                                assert_eq!(cursor.pos(), end as u64);
                                let horizon = offset as u64 + 9 + 512;
                                assert_eq!(
                                    cursor.limit(),
                                    Some(initial.map_or(horizon, |limit| limit.min(horizon)))
                                );
                                assert_eq!(
                                    cursor
                                        .recorded_bits()
                                        .iter()
                                        .map(|b| b.bit)
                                        .collect::<Vec<_>>(),
                                    bits[..end]
                                );
                                (
                                    "normal_handoff",
                                    serde_json::json!({"properties": properties, "complete": complete, "term": term, "extra": extra, "payload": payload, "skip": skip, "children": children.len()}),
                                )
                            } else {
                                let failure = result.unwrap_err();
                                assert!(
                                    matches!(&failure.error, crate::error::ParsingError::Io(message) if message.contains("failed to fill whole buffer") || message.contains("unexpected end of file"))
                                );
                                assert_eq!(
                                    failure.context_stack,
                                    vec!["owner", "Stats", "ExtendedStats", "AlphaV5RunewordExtra"]
                                );
                                assert_eq!(
                                    (failure.bit_offset, failure.context_relative_offset),
                                    (physical.len() as u64, 0)
                                );
                                assert_eq!(
                                    failure.hint.as_deref(),
                                    Some("Possible end of bitstream reached unexpectedly.")
                                );
                                assert_eq!(cursor.pos(), physical.len() as u64);
                                assert_eq!(
                                    cursor.limit(),
                                    if initial.is_none() {
                                        Some(u64::MAX)
                                    } else {
                                        initial
                                    }
                                );
                                assert_eq!(
                                    cursor
                                        .recorded_bits()
                                        .iter()
                                        .map(|b| b.bit)
                                        .collect::<Vec<_>>(),
                                    physical
                                );
                                (
                                    "handled_eof_then_extra_error",
                                    limit_probe_failure(&failure),
                                )
                            };
                            assert_eq!(cursor.context_stack(), vec!["owner"]);
                            depth_witness_state(0, false);
                            let after = limit_probe_snapshot(&cursor);
                            if full {
                                for &bit in &bits[end..] {
                                    assert_eq!(cursor.read_bit().unwrap(), bit);
                                }
                            }
                            rows.push(serde_json::json!({"version": version, "initial": initial, "offset": offset, "trace": trace, "full": full, "v3": v3, "exit_class": class, "before": before, "after": after, "result": value}));
                        }
                    }
                }
            }
        }
    }
    lifetime_probe_report("limit-stats", rows, 96);
}

fn budget_boundary_bits(bits: &[crate::domain::item::RecordedBit]) -> serde_json::Value {
    let start = bits.first().map(|bit| bit.offset);
    let contiguous = bits
        .iter()
        .enumerate()
        .all(|(index, bit)| Some(bit.offset) == start.map(|value| value + index as u64));
    serde_json::json!({"count": bits.len(), "start": start, "contiguous": contiguous,
            "offsets": if contiguous { None } else { Some(bits.iter().map(|bit| bit.offset).collect::<Vec<_>>()) },
            "values": bits.iter().map(|bit| if bit.bit { '1' } else { '0' }).collect::<String>()})
}

fn budget_boundary_item(item: &crate::domain::item::Item) -> serde_json::Value {
    use crate::domain::item::ItemModule;
    let modules: Vec<_> = item.modules.iter().map(|module| match module {
            ItemModule::Opaque(bits) => serde_json::json!({"kind": "Opaque", "bits": bits.iter().map(|bit| if *bit { '1' } else { '0' }).collect::<String>()}),
            ItemModule::Residue(bits) => serde_json::json!({"kind": "Residue", "bits": bits.iter().map(|bit| if *bit { '1' } else { '0' }).collect::<String>()}),
            ItemModule::SemiOpaque { body_bits, reason } => serde_json::json!({"kind": "SemiOpaque", "bits": body_bits.iter().map(|bit| if *bit { '1' } else { '0' }).collect::<String>(), "reason": reason}),
            ItemModule::Sockets { count, items } => serde_json::json!({"kind": "Sockets", "count": count, "items": items.iter().map(budget_boundary_item).collect::<Vec<_>>()}),
            other => serde_json::json!({"kind": "typed", "debug": format!("{other:?}")}),
        }).collect();
    serde_json::json!({"code": item.code, "body_code": item.body.code, "header": format!("{:?}", item.header),
            "body": format!("{:?}", item.body), "version": item.header.version, "flags": item.header.flags,
            "is_compact": item.header.is_compact, "is_socketed": item.header.is_socketed, "is_runeword": item.header.is_runeword,
            "range": item.range, "total_bits": item.total_bits, "logical_width": item.logical_width,
            "bits": budget_boundary_bits(&item.bits), "gap_bits": item.gap_bits, "segments": item.segments,
            "properties": item.properties, "set_attributes": item.set_attributes, "runeword_attributes": item.runeword_attributes,
            "properties_complete": item.properties_complete, "terminator_bit": item.terminator_bit,
            "modules": modules, "children": item.socketed_items.iter().map(budget_boundary_item).collect::<Vec<_>>(),
            "forensic": format!("{:?}", item.forensic_audit), "consumption": format!("{:?}", item.parser_consumption),
            "section_input": format!("{:?}", item.section_parse_input)})
}

#[hypothesis_test]
fn refactor_budget_context_free_actual_fixture_controls() {
    use crate::domain::item::Item;
    let tree = HuffmanTree::new();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut fixtures = Vec::new();
    let mut attempts = Vec::new();
    let mut selected = Vec::new();
    let mut successful_controls = 0;
    for fixture in ["amazon_authority_runeword.d2s", "amazon_10_scrolls.d2s"] {
        let _state = DepthWitnessState::enter(0, false);
        let bytes =
            std::fs::read(root.join("tests/fixtures/savegames/original").join(fixture)).unwrap();
        let stream = crate::domain::item::stream::PlayerItemStream::read(&bytes, &tree)
            .expect("declared source producer must parse");
        let alpha_control = fixture == "amazon_authority_runeword.d2s";
        assert_eq!(
            stream.dialect,
            if alpha_control {
                crate::domain::item::stream::ItemStreamDialect::Alpha
            } else {
                crate::domain::item::stream::ItemStreamDialect::D2rV3
            }
        );
        let items = stream.items;
        assert_eq!(items.len(), if alpha_control { 5 } else { 16 });
        fixtures.push(serde_json::json!({"fixture": fixture, "dialect": stream.dialect, "bytes": bytes.len(), "items": items.iter().map(budget_boundary_item).collect::<Vec<_>>()}));
        let hosts: Vec<_> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                alpha_control
                    && item.header.save_is_alpha
                    && (item.header.is_socketed
                        || item.header.is_runeword
                        || matches!(
                            item.code.trim(),
                            "xrs" | "c8xr" | "rhd" | "wa2" | "ww" | "gcw"
                        ))
            })
            .take(3)
            .collect();
        assert_eq!(hosts.len(), usize::from(alpha_control));
        if alpha_control {
            assert_eq!(hosts[0].0, 4);
            assert_eq!(hosts[0].1.code, "xrs ");
            assert_eq!((hosts[0].1.range.start, hosts[0].1.range.end), (7562, 8306));
        }
        for (index, source) in hosts {
            let start = source.range.start;
            let width = source.range.end.checked_sub(start).unwrap();
            assert!(width > 0 && source.range.end <= (bytes.len() * 8) as u64);
            selected.push(serde_json::json!({"fixture": fixture, "index": index, "code": source.code,
                    "version": source.header.version, "range": source.range, "is_compact": source.header.is_compact}));
            for section_context in [true, false] {
                for depth in [0, 11] {
                    for limit_kind in ["none", "interval", "max"] {
                        let _state = DepthWitnessState::enter(depth, false);
                        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
                        reader.skip(start as u32).unwrap();
                        let mut cursor = BitCursor::new(reader);
                        cursor.set_pos(start);
                        cursor.set_trace(true);
                        let initial = match limit_kind {
                            "interval" => Some(start + width),
                            "max" => Some(u64::MAX),
                            _ => None,
                        };
                        if let Some(limit) = initial {
                            cursor.set_limit(limit);
                        }
                        let before =
                            serde_json::json!({"position": cursor.pos(), "limit": cursor.limit()});
                        let result = Item::from_reader_with_context(
                            &mut cursor,
                            &tree,
                            if section_context {
                                Some((&bytes, start))
                            } else {
                                None
                            },
                            true,
                            index,
                            Some(source.header.is_compact),
                            Some(&source.code),
                        );
                        let (outcome, model, stats_seen, prelude_eligible, opaque_bits) =
                            match result {
                                Ok(item) => {
                                    assert!(
                                        item.bits.iter().all(|bit| bit.offset
                                            < (bytes.len() * 8) as u64
                                            && bit.bit
                                                == (bytes[bit.offset as usize / 8]
                                                    & (1 << (bit.offset % 8))
                                                    != 0)),
                                        "retained raw bits must match declared source"
                                    );
                                    let stats_seen = cursor
                                        .segments()
                                        .iter()
                                        .chain(cursor.active_segments().iter())
                                        .any(|segment| segment.label == "Stats");
                                    if section_context && depth == 0 && stats_seen {
                                        successful_controls += 1;
                                    }
                                    let authority = matches!(
                                        source.code.trim(),
                                        "xrs" | "c8xr" | "rhd" | "wa2" | "ww" | "gcw"
                                    );
                                    let prelude = section_context
                                        && authority
                                        && !item.header.is_compact
                                        && matches!(item.header.version, 0 | 1)
                                        && item.body.code.trim() != "buc";
                                    let opaque_bits: usize = item
                                        .modules
                                        .iter()
                                        .map(|module| match module {
                                            crate::domain::item::ItemModule::Opaque(bits)
                                            | crate::domain::item::ItemModule::Residue(bits) => {
                                                bits.len()
                                            }
                                            crate::domain::item::ItemModule::SemiOpaque {
                                                body_bits,
                                                ..
                                            } => body_bits.len(),
                                            _ => 0,
                                        })
                                        .sum();
                                    (
                                        "ok",
                                        budget_boundary_item(&item),
                                        stats_seen,
                                        prelude,
                                        opaque_bits,
                                    )
                                }
                                Err(failure) => (
                                    "error",
                                    serde_json::json!({"error": failure.error.to_string(), "contexts": failure.context_stack, "bit_offset": failure.bit_offset, "relative_offset": failure.context_relative_offset, "hint": failure.hint}),
                                    cursor
                                        .segments()
                                        .iter()
                                        .chain(cursor.active_segments().iter())
                                        .any(|segment| segment.label == "Stats"),
                                    false,
                                    0,
                                ),
                            };
                        depth_witness_state(depth, false);
                        let expected = match (section_context, depth, limit_kind) {
                            (true, 0, _) => ("ok", 8424, Some(8776), 158, Some(862)),
                            (true, 11, _) => ("ok", 8424, Some(u64::MAX), 574, Some(862)),
                            (false, 0, "interval") => ("error", 8306, Some(8306), 0, None),
                            (false, 0, _) => ("ok", 8424, Some(u64::MAX), 0, Some(862)),
                            (false, 11, "interval") => ("ok", 8306, Some(8306), 568, Some(744)),
                            (false, 11, _) => ("ok", 8424, Some(u64::MAX), 686, Some(862)),
                            _ => unreachable!(),
                        };
                        assert_eq!(
                            (outcome, cursor.pos(), cursor.limit(), opaque_bits, model["total_bits"].as_u64()),
                            expected,
                            "source-pinned continuation contract: context={section_context}, depth={depth}, initial={limit_kind}"
                        );
                        assert!(stats_seen);
                        assert_eq!(prelude_eligible, section_context);
                        let qualified_none_to_max = initial.is_none()
                            && cursor.limit() == Some(u64::MAX)
                            && stats_seen
                            && !prelude_eligible
                            && outcome == "ok";
                        attempts.push(serde_json::json!({"fixture": fixture, "index": index, "source_code": source.code, "source_version": source.header.version,
                                "section_context": section_context, "depth": depth, "limit_kind": limit_kind, "before": before, "after": {"position": cursor.pos(), "limit": cursor.limit(), "contexts": cursor.context_stack(),
                                "recorded": budget_boundary_bits(cursor.recorded_bits()), "segments": cursor.segments(), "active_segments": cursor.active_segments(), "operations": cursor.operation_events()},
                                "outcome": outcome, "model": model, "stats_seen": stats_seen, "authority_prelude_source_eligible": prelude_eligible,
                                "opaque_bits": opaque_bits, "qualified_none_to_max": qualified_none_to_max}));
                    }
                }
            }
        }
    }
    assert_eq!(fixtures.len(), 2);
    assert_eq!(selected.len(), 1);
    assert_eq!(successful_controls, 3);
    assert_eq!(attempts.len(), selected.len() * 12);
    let Ok(output_path) = std::env::var("D2R_BUDGET_ITEM_OUTPUT") else {
        return;
    };
    let output = std::path::PathBuf::from(output_path);
    assert!(output.is_absolute() && !output.exists());
    let report = serde_json::json!({"schema": 1, "fixtures": fixtures, "selected": selected, "attempts": attempts, "successful_controls": successful_controls,
            "scope_limit": "Exactly one source-pinned host and12 replay cells including absent section context, no production change; prelude eligibility is source-backed, not an observed hidden-list limit."});
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
