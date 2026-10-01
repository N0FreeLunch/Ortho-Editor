use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::data::unique_items::UNIQUE_ITEMS;
use d2r_core::domain::stats::parser::read_item_stats_d2r_v3;
use d2r_core::item::{HuffmanTree, Item, ItemQuality};
use d2r_macros::{hypothesis_test, settled_contract_test};
use std::fs;
use std::io::Cursor;

fn read_d2r_v3_items(path: &str) -> (Vec<u8>, Vec<Item>) {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("Could not read {path}: {error}"));
    let huffman = HuffmanTree::new();
    let items = Item::read_player_items_d2r_v3(&bytes, &huffman)
        .unwrap_or_else(|error| panic!("D2R v3 item parsing failed for {path}: {error:?}"));
    (bytes, items)
}

fn source_bits(bytes: &[u8], start: u64, end: u64) -> Vec<bool> {
    (start..end)
        .map(|bit| bytes[bit as usize / 8] & (1 << (bit % 8)) != 0)
        .collect()
}

fn raw_little_endian(bits: &[bool]) -> u32 {
    bits.iter()
        .enumerate()
        .fold(0, |value, (index, bit)| value | (u32::from(*bit) << index))
}

fn reparse_jewelry(bits: &[bool]) -> Item {
    let mut bytes = vec![0; bits.len().div_ceil(8)];
    for (index, bit) in bits.iter().enumerate() {
        if *bit {
            bytes[index / 8] |= 1 << (index % 8);
        }
    }
    let reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
    let mut cursor = BitCursor::new(reader);
    Item::from_reader_with_context_d2r_v3(
        &mut cursor,
        &HuffmanTree::new(),
        Some((&bytes, 0)),
        0,
        0x69,
    )
    .expect("reconstructed jewelry must parse through the explicit dialect")
}

fn without_item_bit_cache(item: &Item) -> Item {
    let mut semantic = item.clone();
    semantic.bits.clear();
    semantic.modules.clear();
    semantic.segments.clear();
    semantic.gap_bits.clear();
    semantic.body.alpha_code_bits.clear();
    semantic.stats.properties.clear();
    semantic.range.start = 0;
    semantic.range.end = 0;
    semantic.total_bits = 0;
    semantic.logical_width = None;
    semantic
}

#[hypothesis_test]
fn d2r_v3_jewelry_manald_rebuild_and_option_bit_witnesses() {
    let (bytes, items) = read_d2r_v3_items(
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Quest2_BloodRaven_Done_PreKashya.d2s",
    );
    let manald = items
        .iter()
        .find(|item| item.range.start == 11320)
        .expect("Manald");
    let semantic = without_item_bit_cache(manald);
    let huffman = HuffmanTree::new();
    let rebuilt = semantic
        .to_bits_d2r_v3_jewelry(&huffman)
        .expect("semantic rebuild");
    let original = source_bits(&bytes, 11320, 11536);
    assert_eq!(rebuilt.len(), 216);
    assert_eq!(
        rebuilt, original,
        "raw-cache-free reconstruction must be bit exact"
    );
    let expected = [
        (62, 7, 0, 6, 6),
        (74, 6, 30, 38, 8),
        (7, 9, 32, 52, 20),
        (27, 8, 0, 20, 20),
    ];
    let mut intervals: Vec<_> = manald
        .properties
        .iter()
        .filter(|property| property.stat_id != 511)
        .map(|property| (property.range.start, property.range.end))
        .collect();
    intervals.sort_unstable();
    assert_eq!(intervals.len(), 4);
    assert_eq!(intervals[0].0, 11458);
    assert_eq!(intervals[3].1, 11524);
    assert!(intervals.windows(2).all(|pair| pair[0].1 == pair[1].0));
    for (stat_id, width, save_add, raw, logical) in expected {
        let property = manald
            .properties
            .iter()
            .find(|property| property.stat_id == stat_id)
            .unwrap();
        let (start, end) = (property.range.start, property.range.end);
        assert_eq!(
            raw_little_endian(&source_bits(&bytes, start, start + 9)),
            stat_id
        );
        assert_eq!(raw_little_endian(&source_bits(&bytes, start + 9, end)), raw);
        assert_eq!(end - start - 9, width);
        assert_eq!(property.value, logical);
        assert_eq!(property.raw_value, raw as i32);
        assert_eq!(property.value + save_add, raw as i32);

        let mut changed = semantic.clone();
        let changed_property = changed
            .properties
            .iter_mut()
            .find(|property| property.stat_id == stat_id)
            .unwrap();
        // Deliberately leave raw_value stale: the writer must use the logical value.
        let new_value = if matches!(stat_id, 62 | 74) {
            logical - 1
        } else {
            logical + 1
        };
        changed_property.value = new_value;
        let mutated = changed
            .to_bits_d2r_v3_jewelry(&huffman)
            .expect("one-value mutation");
        assert_eq!(mutated.len(), original.len());
        let differences: Vec<_> = original
            .iter()
            .zip(&mutated)
            .enumerate()
            .filter_map(|(index, (before, after))| {
                (before != after).then_some(index as u64 + 11320)
            })
            .collect();
        assert!(!differences.is_empty());
        assert!(differences.iter().all(|bit| (start + 9..end).contains(bit)));
        let reparsed = reparse_jewelry(&mutated);
        assert_eq!(reparsed.unique_id, Some(121));
        assert_eq!(reparsed.code.trim(), "rin");
        assert_eq!(reparsed.header.quality, Some(ItemQuality::Unique));
        assert_eq!(reparsed.properties.len(), manald.properties.len());
        for previous in manald
            .properties
            .iter()
            .filter(|property| property.stat_id != 511)
        {
            let actual = reparsed
                .properties
                .iter()
                .find(|property| property.stat_id == previous.stat_id)
                .unwrap();
            let expected_value = if previous.stat_id == stat_id {
                new_value
            } else {
                previous.value
            };
            assert_eq!(
                (actual.param, actual.value),
                (previous.param, expected_value)
            );
        }
        println!("{{\"case\":\"option_bit_witness\",\"stat_id\":{stat_id},\"id_start\":{start},\"value_start\":{},\"end\":{end},\"width\":{width},\"save_add\":{save_add},\"raw\":{raw},\"logical\":{logical},\"mutation_confined\":true}}", start + 9);
    }
    assert_eq!(raw_little_endian(&source_bits(&bytes, 11524, 11533)), 511);
    println!("{{\"case\":\"manald_semantic_reconstruction\",\"source_bits\":216,\"emitted_bits\":216,\"differing_bits\":0,\"raw_item_cache_used\":false}}");
}

#[hypothesis_test]
fn d2r_v3_jewelry_rejects_unsupported_or_lossy_states() {
    let (_, items) = read_d2r_v3_items(
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Quest2_BloodRaven_Done_PreKashya.d2s",
    );
    let item = items
        .iter()
        .find(|item| item.unique_id == Some(121))
        .unwrap();
    let huffman = HuffmanTree::new();
    let mut missing = item.clone();
    missing.body.d2r_v3_auxiliary = None;
    assert!(missing.to_bits_d2r_v3_jewelry(&huffman).is_err());
    let mut unsupported = item.clone();
    unsupported.header.is_socketed = true;
    assert!(unsupported.to_bits_d2r_v3_jewelry(&huffman).is_err());
    let mut wide = item.clone();
    wide.header.x = 16;
    assert!(wide.to_bits_d2r_v3_jewelry(&huffman).is_err());
    let mut overflow = item.clone();
    overflow
        .properties
        .iter_mut()
        .find(|property| property.stat_id == 62)
        .unwrap()
        .value = 128;
    assert!(overflow.to_bits_d2r_v3_jewelry(&huffman).is_err());
    let mut negative = item.clone();
    negative
        .properties
        .iter_mut()
        .find(|property| property.stat_id == 62)
        .unwrap()
        .value = -1;
    assert!(negative.to_bits_d2r_v3_jewelry(&huffman).is_err());
    let mut wrong_padding = item.clone();
    wrong_padding
        .body
        .d2r_v3_auxiliary
        .as_mut()
        .unwrap()
        .alignment_padding = Some(Vec::new());
    assert!(wrong_padding.to_bits_d2r_v3_jewelry(&huffman).is_err());
}

#[hypothesis_test]
fn d2r_v3_jewelry_family_reconstruction_is_bit_exact() {
    let paths = [
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s",
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Quest2_BloodRaven_Done_PreKashya.d2s",
    ];
    let mut verified = 0;
    for path in paths {
        let (bytes, items) = read_d2r_v3_items(path);
        let mut fixture_verified = 0;
        for item in items.iter().filter(|item| {
            matches!(item.code.trim(), "rin" | "amu")
                && item.header.is_identified
                && matches!(
                    item.header.quality,
                    Some(ItemQuality::Normal | ItemQuality::Magic | ItemQuality::Unique)
                )
        }) {
            let rebuilt = without_item_bit_cache(item)
                .to_bits_d2r_v3_jewelry(&HuffmanTree::new())
                .expect("family rebuild");
            assert_eq!(
                rebuilt,
                source_bits(&bytes, item.range.start, item.range.end),
                "{path} at {}",
                item.range.start
            );
            fixture_verified += 1;
        }
        assert!(
            fixture_verified > 0,
            "family fixture must exercise the writer"
        );
        verified += fixture_verified;
    }
    assert!(verified >= 2);
    println!(
        "{{\"case\":\"jewelry_family\",\"fixtures\":2,\"items\":{verified},\"differing_bits\":0}}"
    );
}

#[hypothesis_test]
fn d2r_v3_runeword_min_max_pair_consumes_companion_value() {
    let bytes = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Could not read Beta reference fixture");
    let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
    reader
        .skip(9416)
        .expect("runeword property-list start must be in the fixture");
    let mut cursor = BitCursor::new(reader);
    cursor.set_pos(9416);
    cursor.base_pos = 0;

    let huffman = HuffmanTree::new();
    let (properties, complete, _, _, _, _, _) = read_item_stats_d2r_v3(
        &mut cursor,
        "xrs",
        105,
        None,
        &huffman,
        false,
        Some(ItemQuality::Normal),
        1 << 4,
        true,
        false,
        false,
        false,
        true,
    )
    .expect("the runeword property list must parse");

    assert!(complete, "the property list must reach its terminator");
    assert_eq!(
        properties
            .iter()
            .map(|property| property.stat_id)
            .collect::<Vec<_>>(),
        [17, 18, 83, 198, 201, 511]
    );
    assert_eq!(
        properties
            .iter()
            .take(5)
            .map(|property| property.raw_value)
            .collect::<Vec<_>>(),
        [60, 60, 2, 10, 2]
    );
    assert_eq!(
        properties
            .iter()
            .take(5)
            .map(|property| property.param)
            .collect::<Vec<_>>(),
        [0, 0, 7, 25551, 24778]
    );
    assert_eq!(
        properties
            .iter()
            .take(5)
            .map(|property| (property.range.start, property.range.end))
            .collect::<Vec<_>>(),
        [
            (9416, 9434),
            (9434, 9443),
            (9443, 9458),
            (9458, 9490),
            (9490, 9522),
        ]
    );
    assert_eq!(properties[5].range.start, 9522);
    assert_eq!(properties[5].range.end, 9536);
    assert_eq!(cursor.pos(), 9536);
}

fn assert_exact_raw_spans(items: &[Item]) {
    for item in items {
        let start = item.range.start;
        let end = item.range.end;
        assert_eq!(item.total_bits, end - start);
        assert_eq!(item.logical_width, Some(end - start));
        assert_eq!(item.bits.len() as u64, end - start);
        for (index, bit) in item.bits.iter().enumerate() {
            assert_eq!(bit.offset, start + index as u64);
        }
        assert_exact_raw_spans(&item.socketed_items);
    }
}

fn parse_d2r_v3_item_at(bytes: &[u8], start: u64) -> (Item, u64, Vec<(u64, u64, String)>) {
    let huffman = HuffmanTree::new();
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    reader
        .skip(start as u32)
        .expect("reference start must be in the file");
    let mut cursor = BitCursor::new(reader);
    cursor.set_pos(start);
    cursor.base_pos = 0;
    cursor.set_trace(true);
    let item = match Item::from_reader_with_context_d2r_v3(
        &mut cursor,
        &huffman,
        Some((bytes, start)),
        0,
        105,
    ) {
        Ok(item) => item,
        Err(error) => {
            let active_segments = cursor.active_segments();
            let stats_start = cursor
                .segments()
                .iter()
                .chain(active_segments.iter())
                .find(|segment| segment.label == "Stats")
                .map(|segment| segment.start);
            let (initial_stats_range, initial_stats_segments, initial_stats_events) =
                if let Some(stats_start) = stats_start {
                    let prefix_end = stats_start.saturating_add(68);
                    let initial_stats_segments = cursor
                        .segments()
                        .iter()
                        .chain(active_segments.iter())
                        .filter(|segment| {
                            segment.label.starts_with("Stat(")
                                && segment.start >= stats_start
                                && segment.start <= prefix_end
                        })
                        .take(8)
                        .collect::<Vec<_>>();
                    let initial_stats_events = cursor
                        .operation_events()
                        .iter()
                        .filter(|event| {
                            event.context_stack.last().map(String::as_str) == Some("Stats")
                                && event.pre_read_bit >= stats_start
                                && event.pre_read_bit < prefix_end
                        })
                        .take(32)
                        .collect::<Vec<_>>();
                    (
                        Some((stats_start, prefix_end)),
                        initial_stats_segments,
                        initial_stats_events,
                    )
                } else {
                    (None, Vec::new(), Vec::new())
                };
            let recent_segments = cursor
                .segments()
                .iter()
                .rev()
                .take(8)
                .rev()
                .collect::<Vec<_>>();
            let recent_events = cursor
                .operation_events()
                .iter()
                .rev()
                .take(24)
                .rev()
                .collect::<Vec<_>>();
            panic!(
                "Single D2R v3 item at {start} failed: {error:?}\n\
                 Initial Stats prefix range: {:?}\n\
                 Initial Stats segments ({}): {:#?}\n\
                 Initial Stats bit-read events ({}): {:#?}\n\
                 Cursor bit: {}\n\
                 Context stack: {:#?}\n\
                 Active segments: {:#?}\n\
                 Last {} completed segments: {:#?}\n\
                 Last {} bit-read events: {:#?}",
                initial_stats_range,
                initial_stats_segments.len(),
                initial_stats_segments,
                initial_stats_events.len(),
                initial_stats_events,
                cursor.pos(),
                cursor.context_stack(),
                active_segments,
                recent_segments.len(),
                recent_segments,
                recent_events.len(),
                recent_events,
            );
        }
    };
    cursor
        .byte_align()
        .unwrap_or_else(|error| panic!("Could not byte-align item at {start}: {error:?}"));
    let consumed = cursor.pos() - start;
    let timeline: Vec<_> = cursor
        .segments()
        .iter()
        .map(|segment| (segment.start, segment.end, segment.label.clone()))
        .collect();
    (item, consumed, timeline)
}

#[hypothesis_test]
fn d2r_v3_beta_individual_items_match_pinned_intervals() {
    let bytes = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Could not read Beta reference fixture");
    let expected = [
        (7360, 7440),
        (7440, 7520),
        (7520, 7600),
        (7600, 7680),
        (7680, 7840),
        (7840, 8040),
        (8040, 8240),
        (8240, 8440),
        (8440, 8608),
        (8608, 8792),
        (8792, 8992),
        (8992, 9072),
        (9072, 9240),
        (9240, 9536),
        (9536, 9624),
        (9624, 9712),
        (9712, 9800),
        (9800, 9968),
        (9968, 10136),
    ];
    for (index, (start, expected_end)) in expected.into_iter().enumerate() {
        let (item, consumed, timeline) = parse_d2r_v3_item_at(&bytes, start);
        if index == 9 {
            assert_eq!(item.code.trim(), "jew", "Item {index} code");
            assert!(
                !item.header.is_identified,
                "Item {index} must remain unidentified"
            );
            assert!(
                item.properties.is_empty(),
                "Hidden properties must not become semantic output"
            );
            assert!(
                item.stats.properties.is_empty(),
                "Hidden properties must not enter Stats output"
            );
            let stats_start = timeline
                .iter()
                .find(|(_, _, label)| label == "Stats")
                .map(|(segment_start, _, _)| *segment_start);
            assert_eq!(stats_start, Some(8758), "Item {index} Stats start");
        }
        if (13..=16).contains(&index) {
            let expected_code = ["xrs", "r15", "r13", "r08"][index - 13];
            assert_eq!(item.code.trim(), expected_code, "Wire record {index} code");
        }
        assert_eq!(
            start + consumed,
            expected_end,
            "Item {index} parsed as code {:?} header {:?} quality {:?} unique {:?}; timeline {:?}",
            item.code,
            item.header,
            item.quality,
            item.unique_id,
            timeline
        );
    }
}

#[hypothesis_test]
fn d2r_v3_unidentified_beta_jewel_advances_to_the_pinned_next_item() {
    let bytes = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Could not read Beta reference fixture");
    let (item, consumed, _) = parse_d2r_v3_item_at(&bytes, 8608);

    assert_eq!(item.code.trim(), "jew");
    assert!(!item.header.is_identified);
    assert!(item.properties.is_empty());
    assert!(item.stats.properties.is_empty());
    assert_eq!(8608 + consumed, 8792);
}

#[hypothesis_test]
fn d2r_v3_beta_counted_stream_matches_pinned_item_intervals() {
    let (_, items) =
        read_d2r_v3_items("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s");
    let expected = [
        (7360, 7440),
        (7440, 7520),
        (7520, 7600),
        (7600, 7680),
        (7680, 7840),
        (7840, 8040),
        (8040, 8240),
        (8240, 8440),
        (8440, 8608),
        (8608, 8792),
        (8792, 8992),
        (8992, 9072),
        (9072, 9240),
        (9240, 9800),
        (9800, 9968),
        (9968, 10136),
    ];
    let actual: Vec<_> = items
        .iter()
        .map(|item| (item.range.start, item.range.end))
        .collect();

    assert_eq!(actual, expected);
    assert_eq!(items[9].code.trim(), "jew");
    let runeword = &items[13];
    assert_eq!(runeword.code.trim(), "xrs");
    assert!(runeword.header.is_runeword);
    assert!(runeword.header.is_socketed);
    assert_eq!(runeword.sockets, Some(3));
    assert_eq!(runeword.num_socketed_items, 3);
    assert_eq!(
        runeword
            .socketed_items
            .iter()
            .map(|child| child.code.trim())
            .collect::<Vec<_>>(),
        ["r15", "r13", "r08"]
    );
    assert_eq!(
        runeword
            .socketed_items
            .iter()
            .map(|child| (child.range.start, child.range.end))
            .collect::<Vec<_>>(),
        [(9536, 9624), (9624, 9712), (9712, 9800)]
    );
    assert_exact_raw_spans(&items);
}

fn socket_count_fragment(beta: &[u8], filled: u8, children: usize, next_item: bool) -> Vec<u8> {
    const PARENT_START: usize = 9240 / 8;
    const PARENT_BODY_END: usize = 9536 / 8;
    const CHILD_BYTES: usize = 88 / 8;
    const FILLED_START: usize = 69;
    assert!(filled < 8 && children <= 3);
    let mut parent = beta[PARENT_START..PARENT_BODY_END].to_vec();
    assert_eq!(
        raw_little_endian(&source_bits(&parent, FILLED_START as u64, 72)),
        3,
        "The pinned source field must independently declare three filled sockets"
    );
    for offset in 0..3 {
        let bit = FILLED_START + offset;
        let mask = 1 << (bit % 8);
        parent[bit / 8] = (parent[bit / 8] & !mask) | (((filled >> offset) & 1) << (bit % 8));
    }
    // This is an in-memory counted-list fragment, not a game-loadable save.
    let mut fragment = beta[..8].to_vec();
    fragment.extend_from_slice(b"JM");
    fragment.extend_from_slice(&(if next_item { 2_u16 } else { 1_u16 }).to_le_bytes());
    fragment.extend_from_slice(&parent);
    fragment.extend_from_slice(&beta[PARENT_BODY_END..PARENT_BODY_END + children * CHILD_BYTES]);
    if next_item {
        fragment.extend_from_slice(&beta[7360 / 8..7440 / 8]);
    }
    fragment
}

#[settled_contract_test]
fn d2r_v3_socket_count_controls_child_ownership() {
    let (beta, authority) =
        read_d2r_v3_items("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s");
    assert_eq!(authority[13].socketed_items.len(), 3);
    assert_eq!(authority[13].range.end, 9800);
    for filled in 0..=3_u8 {
        let fragment = socket_count_fragment(&beta, filled, filled as usize, true);
        let items = Item::read_player_items_d2r_v3(&fragment, &HuffmanTree::new())
            .expect("Declared children followed by a top-level item must parse");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].sockets, Some(3));
        assert_eq!(items[0].num_socketed_items, filled);
        assert_eq!(items[0].socketed_items.len(), filled as usize);
        assert_eq!(items[0].range.start, 96);
        let expected_next = 96 + 296 + u64::from(filled) * 88;
        assert_eq!(items[0].range.end, expected_next);
        assert_eq!(items[1].range.start, expected_next);
        assert_eq!(items[1].range.end, expected_next + 80);
        assert_eq!(items[1].code, authority[0].code);
        assert_exact_raw_spans(&items);
        println!(
            "{{\"case\":\"socket_count_ownership\",\"filled\":{filled},\"capacity\":3,\"next_start\":{expected_next}}}"
        );
    }
}

#[settled_contract_test]
fn d2r_v3_socket_count_rejects_missing_children() {
    let beta = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Beta authority must exist");
    for available in 0..3 {
        let fragment = socket_count_fragment(&beta, 3, available, false);
        let result = Item::read_player_items_d2r_v3(&fragment, &HuffmanTree::new());
        assert!(
            result.is_err(),
            "Declared three children but only {available} present must fail, not drop entities"
        );
        let error = result.unwrap_err();
        assert!(
            error.bit_offset >= 392,
            "Failure must identify the child region"
        );
        println!(
            "{{\"case\":\"missing_socket_children\",\"available\":{available},\"rejected\":true}}"
        );
    }
}

#[settled_contract_test]
fn d2r_v3_socket_count_rejects_capacity_and_mode_conflicts() {
    let beta = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Beta authority must exist");
    let overflow = socket_count_fragment(&beta, 4, 3, false);
    let result = Item::read_player_items_d2r_v3(&overflow, &HuffmanTree::new());
    assert!(result.is_err(), "Filled count must not exceed capacity");
    let error = result.unwrap_err();
    assert!(format!("{:?}", error.error).contains("filled socket count"));

    let mut wrong_mode = socket_count_fragment(&beta, 1, 1, false);
    const CHILD_START: usize = 96 + 296;
    // Location six cannot substitute for the header's socket-child mode six.
    for offset in 0..3 {
        let bit = CHILD_START + 35 + offset;
        wrong_mode[bit / 8] &= !(1 << (bit % 8));
    }
    for offset in 0..4 {
        let bit = CHILD_START + 38 + offset;
        let mask = 1 << (bit % 8);
        wrong_mode[bit / 8] = (wrong_mode[bit / 8] & !mask) | (((6_u8 >> offset) & 1) << (bit % 8));
    }
    let result = Item::read_player_items_d2r_v3(&wrong_mode, &HuffmanTree::new());
    assert!(
        result.is_err(),
        "A non-child mode must not become a child through its location"
    );
    let error = result.unwrap_err();
    assert_eq!(error.bit_offset, CHILD_START as u64);
    assert!(format!("{:?}", error.error).contains("socket child mode"));
}

#[hypothesis_test]
fn d2r_v3_same_assassin_pair_recovers_the_cap_to_manald_swap() {
    let (before_bytes, before_items) = read_d2r_v3_items(
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s",
    );
    let (after_bytes, after_items) = read_d2r_v3_items(
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Quest2_BloodRaven_Done_PreKashya.d2s",
    );

    assert_eq!(before_bytes.len(), 1509);
    assert_eq!(after_bytes.len(), 1515);
    assert_eq!(before_items.len(), 33);
    assert_eq!(after_items.len(), 33);

    let cap = before_items
        .iter()
        .find(|item| item.range.start == 8592)
        .expect("Cap target interval should be present");
    assert_eq!(cap.range.end, 8760);
    assert_eq!(cap.code.trim(), "cap");
    assert_eq!(cap.quality, Some(ItemQuality::Normal));

    let manald = after_items
        .iter()
        .find(|item| item.range.start == 11320)
        .expect("Manald target interval should be present");
    assert_eq!(manald.range.end, 11536);
    assert_eq!(manald.code.trim(), "rin");
    assert_eq!(manald.quality, Some(ItemQuality::Unique));
    assert_eq!(manald.unique_id, Some(121));
    assert!(manald.properties_complete);

    let semantic_properties: Vec<_> = manald
        .properties
        .iter()
        .filter(|property| property.stat_id != 511)
        .collect();
    let mut option_vector: Vec<_> = semantic_properties
        .iter()
        .map(|property| (property.stat_id, property.param, property.value))
        .collect();
    option_vector.sort_unstable_by_key(|entry| entry.0);
    assert_eq!(
        option_vector,
        [(7, 0, 20), (27, 0, 20), (62, 0, 6), (74, 0, 8)]
    );
    assert!(semantic_properties
        .iter()
        .all(|property| !property.is_opaque));

    let manald_template = UNIQUE_ITEMS
        .iter()
        .find(|unique| unique.id == 121)
        .expect("Manald Heal template should be present");
    assert_eq!(manald_template.stats.len(), 4);
    let mut unmatched_properties = semantic_properties.clone();
    let mut template_match_count = 0;
    for template_stat in manald_template.stats {
        if let Some(index) = unmatched_properties.iter().position(|property| {
            property.stat_id == template_stat.stat_id
                && property.param == template_stat.param
                && (template_stat.min..=template_stat.max).contains(&property.value)
        }) {
            unmatched_properties.remove(index);
            template_match_count += 1;
        }
    }
    assert_eq!(template_match_count, 4);
    assert!(unmatched_properties.is_empty());

    assert_exact_raw_spans(&before_items);
    assert_exact_raw_spans(&after_items);
}

#[hypothesis_test]
fn d2r_v3_asheara_primary_list_consumes_paired_stats_without_drift() {
    let (_, items) = read_d2r_v3_items(
        "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest1_AshearaTalked.d2s",
    );
    let expected = [
        (7360, 7624),
        (7624, 7816),
        (7816, 8128),
        (8128, 8328),
        (8328, 8504),
        (8504, 8824),
    ];
    let actual: Vec<_> = items
        .iter()
        .take(expected.len())
        .map(|item| (item.range.start, item.range.end))
        .collect();

    assert_eq!(items.len(), 38);
    assert_eq!(actual, expected);
    assert_eq!(items[3].code.trim(), "cm2");
    assert_eq!(items[4].code.trim(), "rin");
    assert_eq!(items[5].code.trim(), "skp");
    assert_eq!(items[5].quality, Some(ItemQuality::Rare));
    assert_eq!((items[11].range.start, items[11].range.end), (9832, 10136));

    let lightning_pair: Vec<_> = items[3]
        .properties
        .iter()
        .filter(|property| matches!(property.stat_id, 50 | 51))
        .map(|property| (property.stat_id, property.range.start, property.range.end))
        .collect();
    assert_eq!(lightning_pair, [(50, 8292, 8307), (51, 8307, 8317)]);

    let poison_pair: Vec<_> = items[11]
        .properties
        .iter()
        .filter(|property| matches!(property.stat_id, 57 | 58 | 59))
        .map(|property| (property.stat_id, property.range.start, property.range.end))
        .collect();
    assert_eq!(
        poison_pair,
        [(57, 10049, 10068), (58, 10068, 10078), (59, 10078, 10087)]
    );
    assert_exact_raw_spans(&items);
}

#[hypothesis_test]
fn d2r_v3_assassin_before_bounded_count_prefix_search() {
    let bytes = fs::read(
        "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_BloodRavenKilled_PreKashya.d2s",
    )
    .expect("Could not read the Assassin before-save fixture");
    let marker = bytes
        .windows(4)
        .position(|window| {
            window.starts_with(b"JM") && u16::from_le_bytes([window[2], window[3]]) == 33
        })
        .expect("Expected the 33-item player list marker");
    let count_offset = marker + 2;

    let mut largest_successful_prefix = 0u16;
    let mut first_failing_prefix = 33u16;
    let mut attempts = 0usize;
    let mut last_success_end = None;
    let mut first_failure = None;
    let mut observations = Vec::new();

    while first_failing_prefix - largest_successful_prefix > 1 {
        attempts += 1;
        assert!(
            attempts <= 6,
            "The bounded prefix search exceeded its budget"
        );
        let candidate_count =
            largest_successful_prefix + (first_failing_prefix - largest_successful_prefix) / 2;
        let mut candidate_bytes = bytes.clone();
        candidate_bytes[count_offset..count_offset + 2]
            .copy_from_slice(&candidate_count.to_le_bytes());

        match Item::read_player_items_d2r_v3(&candidate_bytes, &HuffmanTree::new()) {
            Ok(items) if items.len() == candidate_count as usize => {
                last_success_end = items.last().map(|item| item.range.end);
                observations.push(format!(
                    "count={candidate_count}:pass:end={last_success_end:?}"
                ));
                largest_successful_prefix = candidate_count;
            }
            Ok(items) => {
                observations.push(format!(
                    "count={candidate_count}:unexpected-item-count={}",
                    items.len()
                ));
                first_failing_prefix = candidate_count;
            }
            Err(error) => {
                observations.push(format!("count={candidate_count}:fail@{}", error.bit_offset));
                first_failure = Some((candidate_count, error.bit_offset));
                first_failing_prefix = candidate_count;
            }
        }
    }

    assert_eq!(
        first_failing_prefix,
        largest_successful_prefix + 1,
        "The bounded search must isolate adjacent pass/fail prefix lengths"
    );
    eprintln!(
        "PREFIX_BOUNDARY_RESULT declared=33 max_success={} first_failure={} last_success_end={last_success_end:?} first_failure_detail={first_failure:?} attempts={attempts} observations={observations:?}",
        largest_successful_prefix, first_failing_prefix
    );
}
