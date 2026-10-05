use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::data::unique_items::UNIQUE_ITEMS;
use d2r_core::domain::stats::parser::read_item_stats_d2r_v3;
use d2r_core::item::{HuffmanTree, Item, ItemQuality};
use d2r_macros::{hypothesis_test, invariant_test, settled_contract_test};
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

const ARMOR_FIXTURE: &str = "tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s";

fn assert_stored_item_model(actual: &Item, expected: &Item) {
    assert_eq!(actual.header, expected.header);
    assert_eq!(actual.code.trim(), expected.code.trim());
    let mut actual_body = actual.body.clone();
    let mut expected_body = expected.body.clone();
    actual_body.alpha_code_bits.clear();
    expected_body.alpha_code_bits.clear();
    assert_eq!(actual_body, expected_body);
    assert_eq!(
        (
            actual.defense,
            actual.max_durability,
            actual.current_durability,
            actual.quantity,
            actual.sockets
        ),
        (
            expected.defense,
            expected.max_durability,
            expected.current_durability,
            expected.quantity,
            expected.sockets
        )
    );
    assert_eq!(
        (
            actual.has_multiple_graphics,
            actual.multi_graphics_bits,
            actual.has_class_specific_data,
            actual.class_specific_bits,
            actual.low_high_graphic_bits
        ),
        (
            expected.has_multiple_graphics,
            expected.multi_graphics_bits,
            expected.has_class_specific_data,
            expected.class_specific_bits,
            expected.low_high_graphic_bits
        )
    );
    assert_eq!(
        (
            actual.magic_prefix,
            actual.magic_suffix,
            actual.rare_name_1,
            actual.rare_name_2,
            actual.rare_affixes,
            actual.unique_id,
            actual.runeword_id,
            actual.runeword_level
        ),
        (
            expected.magic_prefix,
            expected.magic_suffix,
            expected.rare_name_1,
            expected.rare_name_2,
            expected.rare_affixes,
            expected.unique_id,
            expected.runeword_id,
            expected.runeword_level
        )
    );
    assert_eq!(
        (
            actual.properties_complete,
            actual.terminator_bit,
            actual.set_list_count
        ),
        (
            expected.properties_complete,
            expected.terminator_bit,
            expected.set_list_count
        )
    );
    let logical = |properties: &[d2r_core::domain::stats::entity::ItemProperty]| {
        properties
            .iter()
            .map(|p| (p.stat_id, p.param, p.value, p.is_opaque, p.is_verified()))
            .collect::<Vec<_>>()
    };
    assert_eq!(logical(&actual.properties), logical(&expected.properties));
    assert_eq!(
        actual
            .set_attributes
            .iter()
            .map(|p| logical(p))
            .collect::<Vec<_>>(),
        expected
            .set_attributes
            .iter()
            .map(|p| logical(p))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        logical(&actual.runeword_attributes),
        logical(&expected.runeword_attributes)
    );
    assert_eq!(actual.socketed_items.len(), expected.socketed_items.len());
    for (a, b) in actual.socketed_items.iter().zip(&expected.socketed_items) {
        assert_eq!(a.range, b.range);
        assert_stored_item_model(a, b);
    }
}

fn read_stored_leaf(bytes: &[u8], start: u64, index: usize) -> (Item, u64) {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    reader.skip(u32::try_from(start).unwrap()).unwrap();
    let mut cursor = BitCursor::new(reader);
    cursor.set_pos(start);
    cursor.base_pos = 0;
    let mut item = Item::from_reader_with_context_d2r_v3(
        &mut cursor,
        &HuffmanTree::new(),
        Some((bytes, start)),
        index,
        0x69,
    )
    .expect("natural explicit-dialect leaf decode");
    let unaligned = cursor.pos();
    assert_eq!((item.range.start, item.range.end), (start, unaligned));
    assert!(!item.header.is_socketed && item.socketed_items.is_empty());
    assert_eq!(
        item.body.d2r_v3_auxiliary.as_ref().unwrap().filled_sockets,
        0
    );
    cursor.byte_align().unwrap();
    let aligned = cursor.pos();
    let padding = source_bits(bytes, unaligned, aligned);
    assert_eq!(padding.len() as u64, (8 - unaligned % 8) % 8);
    item.body
        .d2r_v3_auxiliary
        .as_mut()
        .unwrap()
        .alignment_padding = Some(padding);
    (item, aligned)
}

fn patch_raw_field(bytes: &mut [u8], start: u64, end: u64, value: u32) {
    assert!((1..=31).contains(&(end - start)));
    assert!(u64::from(value) < 1_u64 << (end - start));
    for bit in start..end {
        let mask = 1 << (bit % 8);
        let byte = &mut bytes[bit as usize / 8];
        if value & (1 << (bit - start)) != 0 {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
    }
}

fn change_stored_armor(item: &mut Item, field: &str, raw: u32) {
    match field {
        "defense" => {
            item.body.defense = Some(raw);
            item.defense = Some(raw);
        }
        "maximum" => {
            item.body.max_durability = Some(raw);
            item.max_durability = Some(raw);
        }
        "current" => {
            item.body.current_durability = Some(raw);
            item.current_durability = Some(raw);
        }
        "stat16" => item.properties[0].value -= 1,
        "stat75" => item.properties[1].value -= 1,
        _ => panic!("unknown test field"),
    }
}

#[invariant_test]
fn d2r_v3_armor_reconstruction_and_field_mutations_are_bit_exact() {
    let (bytes, items) = read_d2r_v3_items(ARMOR_FIXTURE);
    let huffman = HuffmanTree::new();
    assert_eq!(items.len(), 16);
    for (index, code, quality, start, end) in [
        (4, "hla", ItemQuality::Normal, 7680, 7840),
        (5, "lgl", ItemQuality::Magic, 7840, 8040),
        (7, "lbt", ItemQuality::High, 8240, 8440),
        (8, "cap", ItemQuality::Low, 8440, 8608),
    ] {
        let expected = &items[index];
        assert_eq!(
            (
                expected.code.trim(),
                expected.header.quality,
                expected.range.start,
                expected.range.end
            ),
            (code, Some(quality), start, end)
        );
        let (leaf, natural_end) = read_stored_leaf(&bytes, start, index);
        assert_eq!(natural_end, end);
        assert_stored_item_model(&leaf, expected);
        if index == 4 || index == 7 {
            let semantic = without_item_bit_cache(&leaf);
            assert_eq!(
                semantic.to_bits_d2r_v3_armor(&huffman).unwrap(),
                source_bits(&bytes, start, end)
            );
        }
    }
    // The stored base maximum is not the computed in-game maximum.
    assert_eq!(
        (
            items[7].body.max_durability,
            items[7].body.current_durability
        ),
        (Some(12), Some(13))
    );
    assert_eq!(
        (
            items[7].properties[1].stat_id,
            items[7].properties[1].raw_value,
            items[7].properties[1].value
        ),
        (75, 34, 14)
    );
    let fields = [
        (4, "defense", 7799, 7810, 34, 35),
        (4, "maximum", 7810, 7818, 28, 29),
        (4, "current", 7818, 7827, 28, 27),
        (7, "defense", 8361, 8372, 14, 15),
        (7, "maximum", 8372, 8380, 12, 13),
        (7, "current", 8380, 8389, 13, 12),
        (7, "stat16", 8399, 8408, 15, 14),
        (7, "stat75", 8417, 8424, 34, 33),
    ];
    let mut combined = bytes.clone();
    let mut desired = items.clone();
    for (index, field, start, end, before, after) in fields {
        assert_eq!(raw_little_endian(&source_bits(&bytes, start, end)), before);
        let target = &items[index];
        let mut semantic = without_item_bit_cache(target);
        let stale_raw: Vec<_> = semantic.properties.iter().map(|p| p.raw_value).collect();
        change_stored_armor(&mut semantic, field, after);
        assert_eq!(
            semantic
                .properties
                .iter()
                .map(|p| p.raw_value)
                .collect::<Vec<_>>(),
            stale_raw
        );
        let emitted = semantic.to_bits_d2r_v3_armor(&huffman).unwrap();
        assert_eq!(emitted.len() as u64, target.range.end - target.range.start);
        let mut patched = bytes.clone();
        patch_raw_field(&mut patched, start, end, after);
        assert_eq!(
            emitted,
            source_bits(&patched, target.range.start, target.range.end)
        );
        assert_eq!(
            source_bits(&patched, 0, start),
            source_bits(&bytes, 0, start)
        );
        assert_eq!(
            source_bits(&patched, end, bytes.len() as u64 * 8),
            source_bits(&bytes, end, bytes.len() as u64 * 8)
        );
        let (actual, natural_end) = read_stored_leaf(&patched, target.range.start, index);
        assert_eq!(natural_end, target.range.end);
        assert_stored_item_model(&actual, &semantic);
        let (next, next_end) = read_stored_leaf(&patched, natural_end, index + 1);
        assert_eq!(next_end, items[index + 1].range.end);
        assert_stored_item_model(&next, &items[index + 1]);
        change_stored_armor(&mut desired[index], field, after);
        patch_raw_field(&mut combined, start, end, after);
    }
    let reparsed = Item::read_player_items_d2r_v3(&combined, &huffman).unwrap();
    assert_eq!(reparsed.len(), 16);
    for (actual, expected) in reparsed.iter().zip(&desired) {
        assert_eq!(actual.range, expected.range);
        assert_stored_item_model(actual, expected);
    }
    println!(
        "Armor reconstruction verified:2 originals;8 mutations;20 leaf decodes;16 combined primary seams."
    );
}

#[settled_contract_test]
fn d2r_v3_armor_rejects_unsupported_or_lossy_states() {
    let (_, items) = read_d2r_v3_items(ARMOR_FIXTURE);
    let huffman = HuffmanTree::new();
    let mut rejected = 0;
    let mut reject = |item: Item| {
        let error = item
            .to_bits_d2r_v3_armor(&huffman)
            .expect_err("unsupported armor must not silently fall back");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        rejected += 1;
    };
    for index in [4, 7] {
        let semantic = without_item_bit_cache(&items[index]);
        let mut bad = semantic.clone();
        bad.body.defense = None;
        reject(bad);
        let mut bad = semantic.clone();
        let overflow = 1 << d2r_core::domain::stats::stat_save_bits(31).unwrap();
        bad.body.defense = Some(overflow);
        bad.defense = Some(overflow);
        reject(bad);
        let mut bad = semantic.clone();
        bad.body.current_durability = None;
        reject(bad);
        let mut bad = semantic.clone();
        bad.header.quality = Some(ItemQuality::Magic);
        reject(bad);
        let mut bad = semantic.clone();
        bad.quantity = Some(1);
        reject(bad);
        let mut bad = semantic.clone();
        bad.properties.pop();
        reject(bad);
        let mut bad = semantic.clone();
        bad.body.max_durability = Some(0);
        bad.max_durability = Some(0);
        reject(bad);
    }
    let mut bad = without_item_bit_cache(&items[7]);
    bad.low_high_graphic_bits = None;
    reject(bad);
    let mut bad = without_item_bit_cache(&items[7]);
    bad.properties[0].is_opaque = true;
    reject(bad);
    let mut bad = without_item_bit_cache(&items[7]);
    bad.low_high_graphic_bits = Some(8);
    reject(bad);
    let mut bad = without_item_bit_cache(&items[4]);
    bad.low_high_graphic_bits = Some(1);
    reject(bad);
    let mut bad = without_item_bit_cache(&items[4]);
    bad.current_durability = Some(29);
    reject(bad);
    let mut bad = without_item_bit_cache(&items[4]);
    bad.body.d2r_v3_auxiliary.as_mut().unwrap().realm_data = vec![0; 4];
    reject(bad);
    assert_eq!(rejected, 20);
    println!("Armor rejection contract verified:20 InvalidInput controls.");
}

#[invariant_test]
fn d2r_v3_rare_jewelry_reconstruction_and_option_witnesses() {
    use d2r_core::domain::stats::entity::ItemProperty;
    let path = "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest1_AshearaTalked.d2s";
    let (bytes, items) = read_d2r_v3_items(path);
    let huffman = HuffmanTree::new();
    assert_eq!(items.len(), 38);
    let ring = &items[12];
    assert_eq!(ring.code.trim(), "rin");
    assert_eq!(ring.header.quality, Some(ItemQuality::Rare));
    assert_eq!((ring.range.start, ring.range.end), (10136, 10400));
    assert_eq!(items[13].range.start, 10400);
    assert_eq!((ring.rare_name_1, ring.rare_name_2), (Some(169), Some(94)));
    assert_eq!(
        ring.rare_affixes,
        [Some(389), Some(300), None, Some(379), None, Some(182)]
    );
    for (start, end, value) in [(10260, 10268, 169), (10268, 10276, 94)] {
        assert_eq!(raw_little_endian(&source_bits(&bytes, start, end)), value);
    }
    for (slot, start) in [10276, 10288, 10300, 10301, 10313, 10314]
        .into_iter()
        .enumerate()
    {
        let affix = ring.rare_affixes[slot];
        assert_eq!(source_bits(&bytes, start, start + 1), vec![affix.is_some()]);
        if let Some(value) = affix {
            assert_eq!(
                raw_little_endian(&source_bits(&bytes, start + 1, start + 12)),
                u32::from(value)
            );
        }
    }
    let semantic = without_item_bit_cache(ring);
    let original = source_bits(&bytes, 10136, 10400);
    let ranges: Vec<_> = items
        .iter()
        .map(|item| (item.range.start, item.range.end))
        .collect();
    let verify = |changed: &Item, expected: &[bool], properties: &[ItemProperty]| {
        let emitted = changed
            .to_bits_d2r_v3_jewelry(&huffman)
            .expect("Rare rebuild");
        assert_eq!(emitted.len(), 264);
        assert_eq!(
            emitted, expected,
            "semantic reconstruction must match independently expected bits"
        );
        let local = reparse_jewelry(&emitted);
        // The leaf reader stops before the counted-list owner's alignment field.
        assert_eq!((local.range.start, local.range.end), (0, 263));
        assert_eq!(local.total_bits, 263);
        assert_eq!(
            local
                .body
                .d2r_v3_auxiliary
                .as_ref()
                .unwrap()
                .alignment_padding,
            None
        );
        assert_eq!(
            ring.body
                .d2r_v3_auxiliary
                .as_ref()
                .unwrap()
                .alignment_padding,
            Some(vec![original[263]])
        );
        assert_eq!(local.header, ring.header);
        assert_eq!(
            (local.rare_name_1, local.rare_name_2),
            (changed.rare_name_1, changed.rare_name_2)
        );
        assert_eq!(local.rare_affixes, changed.rare_affixes);
        assert_eq!(
            local
                .properties
                .iter()
                .map(|p| (p.stat_id, p.param, p.raw_value, p.value))
                .collect::<Vec<_>>(),
            properties
                .iter()
                .map(|p| (p.stat_id, p.param, p.raw_value, p.value))
                .collect::<Vec<_>>()
        );
        let mut patched = bytes.clone();
        for (index, value) in emitted.iter().enumerate() {
            let bit = 10136 + index;
            let mask = 1 << (bit % 8);
            patched[bit / 8] = (patched[bit / 8] & !mask) | if *value { mask } else { 0 };
        }
        // Grammar witness only; no checksum-valid save or game-load assertion.
        let full_diff: Vec<_> = (0..bytes.len() * 8)
            .filter(|bit| (bytes[bit / 8] ^ patched[bit / 8]) & (1 << (bit % 8)) != 0)
            .collect();
        let expected_diff: Vec<_> = original
            .iter()
            .zip(expected)
            .enumerate()
            .filter_map(|(bit, (a, b))| (a != b).then_some(10136 + bit))
            .collect();
        assert_eq!(full_diff, expected_diff);
        let readback = Item::read_player_items_d2r_v3(&patched, &huffman).unwrap();
        assert_eq!(
            readback
                .iter()
                .map(|item| (item.range.start, item.range.end))
                .collect::<Vec<_>>(),
            ranges
        );
        let actual = &readback[12];
        assert_eq!(actual.header, ring.header);
        assert_eq!(
            (actual.rare_name_1, actual.rare_name_2),
            (changed.rare_name_1, changed.rare_name_2)
        );
        assert_eq!(actual.rare_affixes, changed.rare_affixes);
        assert_eq!(actual.properties, properties);
        assert_eq!(actual.body.d2r_v3_auxiliary, ring.body.d2r_v3_auxiliary);
        assert_eq!(actual.set_attributes, ring.set_attributes);
        assert_eq!(actual.unique_id, ring.unique_id);
    };
    verify(&semantic, &original, &ring.properties);
    for (id, start, end, raw, logical, new_value, changed_bit) in [
        (0, 10328, 10345, 35, 3, 2, 10337),
        (1, 10345, 10361, 36, 4, 5, 10354),
        (41, 10361, 10379, 206, 6, 7, 10370),
        (118, 10379, 10389, 1, 1, 0, 10388),
    ] {
        assert_eq!(
            raw_little_endian(&source_bits(&bytes, start, start + 9)),
            id
        );
        assert_eq!(
            raw_little_endian(&source_bits(&bytes, start + 9, end)),
            raw as u32
        );
        let mut changed = semantic.clone();
        let property = changed
            .properties
            .iter_mut()
            .find(|p| p.stat_id == id)
            .unwrap();
        assert_eq!(
            (
                property.value,
                property.raw_value,
                property.range.start,
                property.range.end
            ),
            (logical, raw, start, end)
        );
        // Keep raw_value stale: only the logical field drives the emitted value.
        property.value = new_value;
        let mut expected = original.clone();
        expected[(changed_bit - 10136) as usize] ^= true;
        let mut properties = ring.properties.clone();
        let expected_property = properties.iter_mut().find(|p| p.stat_id == id).unwrap();
        expected_property.value = new_value;
        expected_property.raw_value += new_value - logical;
        verify(&changed, &expected, &properties);
    }
    let identity_bits = |item: &Item| {
        let mut bits = Vec::new();
        for value in [item.rare_name_1.unwrap(), item.rare_name_2.unwrap()] {
            bits.extend((0..8).map(|bit| value & (1 << bit) != 0));
        }
        for affix in item.rare_affixes {
            bits.push(affix.is_some());
            if let Some(value) = affix {
                bits.extend((0..11).map(|bit| value & (1 << bit) != 0));
            }
        }
        bits
    };
    assert_eq!(identity_bits(&semantic), source_bits(&bytes, 10260, 10326));
    let mut counterfactuals = Vec::new();
    for name in 0..2 {
        let mut changed = semantic.clone();
        let target = if name == 0 {
            &mut changed.rare_name_1
        } else {
            &mut changed.rare_name_2
        };
        *target = Some(target.unwrap() ^ 1);
        counterfactuals.push(changed);
    }
    for slot in [0, 1, 3, 5] {
        let mut changed = semantic.clone();
        changed.rare_affixes[slot] = Some(changed.rare_affixes[slot].unwrap() ^ 1);
        counterfactuals.push(changed);
    }
    let mut zero = semantic.clone();
    zero.rare_affixes[0] = Some(0);
    counterfactuals.push(zero);
    for absent in [2, 4] {
        let mut changed = semantic.clone();
        changed.rare_affixes[0] = None;
        changed.rare_affixes[absent] = Some(0);
        counterfactuals.push(changed);
    }
    for changed in counterfactuals {
        let mut expected = original.clone();
        expected[124..190].copy_from_slice(&identity_bits(&changed));
        verify(&changed, &expected, &ring.properties);
    }
    assert_eq!(
        fs::read(path).unwrap(),
        bytes,
        "fixture must remain immutable"
    );
}

#[settled_contract_test]
fn d2r_v3_rare_jewelry_rejects_inconsistent_identity() {
    use d2r_core::domain::stats::entity::StatMaturityTier;
    let (_, items) = read_d2r_v3_items(
        "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest1_AshearaTalked.d2s",
    );
    let ring = without_item_bit_cache(&items[12]);
    let huffman = HuffmanTree::new();
    let reject = |item: Item| {
        assert_eq!(
            item.to_bits_d2r_v3_jewelry(&huffman).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        )
    };
    for name in 0..2 {
        let mut missing = ring.clone();
        if name == 0 {
            missing.rare_name_1 = None;
        } else {
            missing.rare_name_2 = None;
        }
        reject(missing);
    }
    for slot in 0..6 {
        let mut overflow = ring.clone();
        overflow.rare_affixes[slot] = Some(2048);
        reject(overflow);
    }
    let mut crafted = ring.clone();
    crafted.header.quality = Some(ItemQuality::Crafted);
    reject(crafted);
    let mut opaque = ring.clone();
    opaque.properties[0].is_opaque = true;
    reject(opaque);
    let mut unverified = ring.clone();
    unverified.properties[0].maturity = StatMaturityTier::Inferred;
    reject(unverified);
    let mut paired = ring.clone();
    paired.properties[0].stat_id = 17;
    reject(paired);
    let mut missing_terminal = ring.clone();
    missing_terminal.properties.pop();
    reject(missing_terminal);
    let mut premature = ring.clone();
    premature
        .properties
        .insert(0, ring.properties.last().unwrap().clone());
    reject(premature);
}

#[settled_contract_test]
fn d2r_v3_set_presence_mask_provenance() {
    let path = "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest1_AshearaTalked.d2s";
    let (original, baseline) = read_d2r_v3_items(path);
    let huffman = HuffmanTree::new();
    assert_eq!(baseline.len(), 38);
    let ring = &baseline[10];
    assert_eq!(ring.code.trim(), "rin");
    assert_eq!(ring.header.quality, Some(ItemQuality::Set));
    assert_eq!(ring.unique_id, Some(29));
    assert_eq!((ring.range.start, ring.range.end), (9616, 9832));
    assert_eq!(baseline[11].range.start, 9832);
    assert_eq!(raw_little_endian(&source_bits(&original, 9754, 9759)), 1);
    assert_eq!(ring.set_list_count, 1);
    assert_eq!(ring.set_attributes.len(), 1);
    assert_eq!(
        ring.body
            .d2r_v3_auxiliary
            .as_ref()
            .unwrap()
            .set_property_flags,
        Some([true, false, false, false, false])
    );
    assert_eq!(
        baseline[4]
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .unwrap()
            .set_property_flags,
        None
    );
    assert_eq!(
        without_item_bit_cache(ring)
            .to_bits_d2r_v3_jewelry(&huffman)
            .unwrap(),
        source_bits(&original, 9616, 9832),
        "preserved Set fields must now support exact reconstruction"
    );
    let ranges: Vec<_> = baseline
        .iter()
        .map(|item| (item.range.start, item.range.end))
        .collect();
    let mut seen = Vec::new();
    // Counterfactuals exercise the reader grammar only, not checksum or game-load validity.
    for mask in [1_u8, 2, 16] {
        let mut bytes = original.clone();
        let flags = std::array::from_fn(|index| mask & (1 << index) != 0);
        for (index, flag) in flags.iter().enumerate() {
            let bit = 9754 + index;
            let bit_mask = 1 << (bit % 8);
            bytes[bit / 8] = (bytes[bit / 8] & !bit_mask) | if *flag { bit_mask } else { 0 };
        }
        assert_eq!(
            raw_little_endian(&source_bits(&bytes, 9754, 9759)),
            u32::from(mask)
        );
        let changed_bits: Vec<_> = (0..original.len() * 8)
            .filter(|bit| (original[bit / 8] ^ bytes[bit / 8]) & (1 << (bit % 8)) != 0)
            .collect();
        assert_eq!(changed_bits.len(), if mask == 1 { 0 } else { 2 });
        assert!(changed_bits.iter().all(|bit| (9754..9759).contains(bit)));
        let items = Item::read_player_items_d2r_v3(&bytes, &huffman).unwrap();
        assert_eq!(items.len(), 38);
        assert_eq!(
            items
                .iter()
                .map(|item| (item.range.start, item.range.end))
                .collect::<Vec<_>>(),
            ranges
        );
        let actual = &items[10];
        assert_eq!(actual.header, ring.header);
        assert_eq!(actual.unique_id, ring.unique_id);
        assert_eq!(actual.set_list_count, ring.set_list_count);
        assert_eq!(actual.properties, ring.properties);
        assert_eq!(actual.set_attributes, ring.set_attributes);
        let retained = actual
            .body
            .d2r_v3_auxiliary
            .as_ref()
            .unwrap()
            .set_property_flags;
        assert_eq!(retained, Some(flags));
        assert!(
            !seen.contains(&retained),
            "equal-popcount masks must remain distinct"
        );
        seen.push(retained);
        let mut expected_auxiliary = ring.body.d2r_v3_auxiliary.clone();
        expected_auxiliary.as_mut().unwrap().set_property_flags = Some(flags);
        assert_eq!(actual.body.d2r_v3_auxiliary, expected_auxiliary);
    }
    assert_eq!(
        fs::read(path).unwrap(),
        original,
        "fixture must remain immutable"
    );
}

#[invariant_test]
fn d2r_v3_set_jewelry_reconstruction_and_option_witnesses() {
    let path = "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest1_AshearaTalked.d2s";
    let (bytes, items) = read_d2r_v3_items(path);
    let huffman = HuffmanTree::new();
    let ring = &items[10];
    let semantic = without_item_bit_cache(ring);
    let original = source_bits(&bytes, 9616, 9832);
    let rebuilt = semantic
        .to_bits_d2r_v3_jewelry(&huffman)
        .expect("Set rebuild");
    assert_eq!(rebuilt.len(), 216);
    assert_eq!(rebuilt, original, "Set reconstruction must be bit exact");
    assert_eq!(ring.unique_id, Some(29));
    let ranges: Vec<_> = items
        .iter()
        .map(|item| (item.range.start, item.range.end))
        .collect();
    assert_eq!(items.len(), 38);
    assert_eq!(items[11].range.start, 9832);
    for mask in [1_u8, 2, 16] {
        let mut changed = semantic.clone();
        let flags = std::array::from_fn(|index| mask & (1 << index) != 0);
        changed
            .body
            .d2r_v3_auxiliary
            .as_mut()
            .unwrap()
            .set_property_flags = Some(flags);
        let actual = changed.to_bits_d2r_v3_jewelry(&huffman).unwrap();
        let mut expected = original.clone();
        expected[138..143].copy_from_slice(&flags);
        assert_eq!(
            actual, expected,
            "mask position must not be synthesized from count"
        );
        let reparsed = reparse_jewelry(&actual);
        assert_eq!(
            reparsed
                .body
                .d2r_v3_auxiliary
                .as_ref()
                .unwrap()
                .set_property_flags,
            Some(flags)
        );
        assert_eq!(reparsed.set_list_count, 1);
        assert_eq!(reparsed.set_attributes.len(), 1);
        assert_eq!(reparsed.unique_id, Some(29));
    }
    for (extra, stat_id, start, end, logical, raw, changed_bit) in [
        (false, 34, 9759, 9774, 2, 2, 9768),
        (false, 60, 9774, 9790, 6, 6, 9783),
        (true, 0, 9799, 9816, 10, 42, 9808),
    ] {
        let mut changed = semantic.clone();
        let list = if extra {
            &mut changed.set_attributes[0]
        } else {
            &mut changed.properties
        };
        let property = list
            .iter_mut()
            .find(|property| property.stat_id == stat_id)
            .unwrap();
        assert_eq!((property.range.start, property.range.end), (start, end));
        assert_eq!((property.value, property.raw_value), (logical, raw));
        assert_eq!(
            raw_little_endian(&source_bits(&bytes, start, start + 9)),
            stat_id
        );
        assert_eq!(
            raw_little_endian(&source_bits(&bytes, start + 9, end)),
            raw as u32
        );
        // Leave raw_value stale to prove that logical fields drive reconstruction.
        property.value += 1;
        let emitted = changed.to_bits_d2r_v3_jewelry(&huffman).unwrap();
        assert_eq!(emitted.len(), 216);
        let diff: Vec<_> = original
            .iter()
            .zip(&emitted)
            .enumerate()
            .filter_map(|(bit, (before, after))| (before != after).then_some(bit as u64 + 9616))
            .collect();
        assert_eq!(diff, vec![changed_bit]);
        let mut patched = bytes.clone();
        for (index, value) in emitted.iter().enumerate() {
            let bit = 9616 + index;
            let bit_mask = 1 << (bit % 8);
            patched[bit / 8] = (patched[bit / 8] & !bit_mask) | if *value { bit_mask } else { 0 };
        }
        // In-memory grammar witness only; not a checksum-valid or game-loaded save.
        let whole_diff: Vec<_> = (0..bytes.len() * 8)
            .filter(|bit| (bytes[bit / 8] ^ patched[bit / 8]) & (1 << (bit % 8)) != 0)
            .collect();
        assert_eq!(whole_diff, vec![changed_bit as usize]);
        let readback = Item::read_player_items_d2r_v3(&patched, &huffman).unwrap();
        assert_eq!(
            readback
                .iter()
                .map(|item| (item.range.start, item.range.end))
                .collect::<Vec<_>>(),
            ranges
        );
        let actual = &readback[10];
        assert_eq!(actual.header, ring.header);
        assert_eq!(actual.unique_id, ring.unique_id);
        assert_eq!(actual.body.d2r_v3_auxiliary, ring.body.d2r_v3_auxiliary);
        let mut expected = ring.clone();
        let expected_list = if extra {
            &mut expected.set_attributes[0]
        } else {
            &mut expected.properties
        };
        let expected_property = expected_list
            .iter_mut()
            .find(|property| property.stat_id == stat_id)
            .unwrap();
        expected_property.value += 1;
        expected_property.raw_value += 1;
        assert_eq!(actual.properties, expected.properties);
        assert_eq!(actual.set_attributes, expected.set_attributes);
        println!(
            "{{\"case\":\"set_option_witness\",\"stat_id\":{stat_id},\"value_start\":{},\"end\":{end},\"changed_bit\":{changed_bit}}}",
            start + 9
        );
    }
    assert_eq!(fs::read(path).unwrap(), bytes, "fixture must be immutable");
}

#[settled_contract_test]
fn d2r_v3_set_jewelry_rejects_inconsistent_lists() {
    use d2r_core::domain::stats::entity::StatMaturityTier;
    let (_, items) = read_d2r_v3_items(
        "tests/fixtures/savegames/gameplay/normal/act3/TESTDRUID_Quest1_AshearaTalked.d2s",
    );
    let ring = without_item_bit_cache(&items[10]);
    let huffman = HuffmanTree::new();
    let reject = |item: Item| {
        assert_eq!(
            item.to_bits_d2r_v3_jewelry(&huffman).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    };
    let mut missing = ring.clone();
    missing
        .body
        .d2r_v3_auxiliary
        .as_mut()
        .unwrap()
        .set_property_flags = None;
    reject(missing);
    let mut mask = ring.clone();
    mask.body
        .d2r_v3_auxiliary
        .as_mut()
        .unwrap()
        .set_property_flags = Some([false; 5]);
    reject(mask);
    let mut count = ring.clone();
    count.set_list_count = 2;
    reject(count);
    let mut absent = ring.clone();
    absent.set_attributes.clear();
    reject(absent);
    let mut additional = ring.clone();
    additional
        .set_attributes
        .push(ring.set_attributes[0].clone());
    reject(additional);
    let mut empty = ring.clone();
    empty.set_attributes[0].clear();
    reject(empty);
    let mut terminal = ring.clone();
    terminal.set_attributes[0].pop();
    reject(terminal);
    let mut premature = ring.clone();
    premature.set_attributes[0].insert(0, ring.set_attributes[0][1].clone());
    reject(premature);
    let mut opaque = ring.clone();
    opaque.set_attributes[0][0].is_opaque = true;
    reject(opaque);
    let mut unverified = ring.clone();
    unverified.set_attributes[0][0].maturity = StatMaturityTier::Inferred;
    reject(unverified);
    let mut paired = ring.clone();
    paired.set_attributes[0][0].stat_id = 17;
    reject(paired);
    let mut no_id = ring.clone();
    no_id.unique_id = None;
    reject(no_id);
    let mut overflow_id = ring.clone();
    overflow_id.unique_id = Some(4096);
    reject(overflow_id);
    let magic = without_item_bit_cache(&items[4]);
    let mut non_set_mask = magic.clone();
    non_set_mask
        .body
        .d2r_v3_auxiliary
        .as_mut()
        .unwrap()
        .set_property_flags = Some([false; 5]);
    reject(non_set_mask);
    let mut non_set_count = magic.clone();
    non_set_count.set_list_count = 1;
    reject(non_set_count);
    let mut non_set_list = magic;
    non_set_list.set_attributes = ring.set_attributes.clone();
    reject(non_set_list);
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
        println!(
            "{{\"case\":\"option_bit_witness\",\"stat_id\":{stat_id},\"id_start\":{start},\"value_start\":{},\"end\":{end},\"width\":{width},\"save_add\":{save_add},\"raw\":{raw},\"logical\":{logical},\"mutation_confined\":true}}",
            start + 9
        );
    }
    assert_eq!(raw_little_endian(&source_bits(&bytes, 11524, 11533)), 511);
    println!(
        "{{\"case\":\"manald_semantic_reconstruction\",\"source_bits\":216,\"emitted_bits\":216,\"differing_bits\":0,\"raw_item_cache_used\":false}}"
    );
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

#[settled_contract_test]
fn d2r_v3_player_list_keeps_empty_primary_ownership() {
    let beta = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Beta authority must exist");
    assert_eq!(&beta[884..886], b"if");
    assert_eq!(&beta[916..920], b"JM\x10\x00");
    // Preserve the real prefix; the two list payloads are an in-memory counterexample.
    let mut candidate = beta[..916].to_vec();
    candidate.extend_from_slice(b"JM\x00\x00JM\x01\x00");
    candidate.extend_from_slice(&beta[7360 / 8..7440 / 8]);
    let items = Item::read_player_items_d2r_v3(&candidate, &HuffmanTree::new())
        .expect("An empty owned primary list is valid");
    assert_eq!(
        items.len(),
        0,
        "Secondary items do not belong to the player list"
    );
    println!(
        "{{\"case\":\"empty_primary_ownership\",\"returned_items\":0,\"primary_offset\":916}}"
    );
}

#[settled_contract_test]
fn d2r_v3_player_list_does_not_hide_primary_failure() {
    let beta = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Beta authority must exist");
    let mut candidate = beta[..916].to_vec();
    candidate.extend_from_slice(b"JM\x01\x00");
    // This parent declares three children, but its body is followed by another list.
    candidate.extend_from_slice(&beta[9240 / 8..9536 / 8]);
    candidate.extend_from_slice(b"JM\x01\x00");
    candidate.extend_from_slice(&beta[7360 / 8..7440 / 8]);
    let result = Item::read_player_items_d2r_v3(&candidate, &HuffmanTree::new());
    assert!(
        result.is_err(),
        "A later valid list must not hide the primary error"
    );
    let error = result.unwrap_err();
    assert!(
        error.bit_offset >= 7656,
        "Keep the original child failure location"
    );
    println!(
        "{{\"case\":\"primary_failure_not_hidden\",\"rejected\":true,\"offset\":{}}}",
        error.bit_offset
    );
}

#[settled_contract_test]
fn d2r_v3_player_list_ignores_header_and_skill_marker_lookalikes() {
    let (beta, expected) =
        read_d2r_v3_items("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s");
    let mut candidate = beta.clone();
    for start in [100, 886] {
        candidate[start..start + 4].copy_from_slice(b"JM\x01\x00");
        candidate[start + 4..start + 14].copy_from_slice(&beta[7360 / 8..7440 / 8]);
    }
    let actual = Item::read_player_items_d2r_v3(&candidate, &HuffmanTree::new())
        .expect("Marker lookalikes outside the owned player list must be ignored");
    assert_eq!(actual.len(), 16);
    for (parsed, original) in actual.iter().zip(&expected) {
        assert_eq!(
            (parsed.range.start, parsed.range.end),
            (original.range.start, original.range.end)
        );
        assert_eq!(parsed.code, original.code);
        assert_eq!(parsed.bits, original.bits);
    }
    println!(
        "{{\"case\":\"prefix_marker_lookalikes\",\"returned_items\":16,\"first_start\":7360}}"
    );
}

#[settled_contract_test]
fn d2r_v3_player_list_rejects_unowned_or_unsupported_prefix() {
    let beta = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Beta authority must exist");
    let huffman = HuffmanTree::new();
    for (field, offset, replacement) in [
        ("save magic", 0, &b"XXXX"[..]),
        ("prefix version", 4, &106_u32.to_le_bytes()[..]),
        ("attribute marker", 833, &b"xx"[..]),
        ("skill marker", 884, &b"xx"[..]),
        ("counted list marker", 916, &b"xx"[..]),
    ] {
        let mut candidate = beta.clone();
        candidate[offset..offset + replacement.len()].copy_from_slice(replacement);
        let result = Item::read_player_items_d2r_v3(&candidate, &huffman);
        assert!(
            result.is_err(),
            "Invalid {field} must not recover through another list"
        );
        assert!(format!("{:?}", result.unwrap_err().error).contains(field));
    }
    let mut unknown_id = beta.clone();
    for offset in 0..9 {
        let bit = 835 * 8 + offset;
        let mask = 1 << (bit % 8);
        unknown_id[bit / 8] =
            (unknown_id[bit / 8] & !mask) | (((85_u32 >> offset) as u8 & 1) << (bit % 8));
    }
    let result = Item::read_player_items_d2r_v3(&unknown_id, &huffman);
    assert!(
        result.is_err(),
        "Unverified character IDs must not yield a guessed prefix"
    );
    let error = result.unwrap_err();
    assert_eq!(error.bit_offset, 6680);
    assert!(format!("{:?}", error.error).contains("character attribute ID"));
    for length in [0, 7, 834, 835, 883, 918] {
        assert!(Item::read_player_items_d2r_v3(&beta[..length], &huffman).is_err());
    }
    println!(
        "{{\"case\":\"invalid_player_prefix\",\"marker_or_version_cases\":5,\"unknown_id_rejected\":true,\"truncation_cases\":6}}"
    );
}

#[settled_contract_test]
fn d2r_v3_explicit_list_requires_the_supplied_boundary() {
    let beta = fs::read("tests/fixtures/savegames/beta/amazon_v105_slice2_equipment.d2s")
        .expect("Beta authority must exist");
    let huffman = HuffmanTree::new();
    let fragment = b"JM\x00\x00JM\x01\x00";
    assert!(
        Item::read_counted_list_d2r_v3(fragment, 0, &huffman, 0x69)
            .unwrap()
            .is_empty()
    );
    assert!(Item::read_player_items_d2r_v3(fragment, &huffman).is_err());
    for offset in [usize::MAX, fragment.len(), 1, 4] {
        assert!(Item::read_counted_list_d2r_v3(fragment, offset, &huffman, 0x69).is_err());
    }
    for truncated_header in [&b""[..], &b"J"[..], &b"JM"[..], &b"JM\x00"[..]] {
        assert!(Item::read_counted_list_d2r_v3(truncated_header, 0, &huffman, 0x69).is_err());
    }
    let items = Item::read_counted_list_d2r_v3(&beta, 916, &huffman, 0x69)
        .expect("An independently pinned list offset must retain source ranges");
    assert_eq!(items.len(), 16);
    assert_eq!(items[0].range.start, 7360);
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
        let items = Item::read_counted_list_d2r_v3(&fragment, 8, &HuffmanTree::new(), 0x69)
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
        let result = Item::read_counted_list_d2r_v3(&fragment, 8, &HuffmanTree::new(), 0x69);
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
    let result = Item::read_counted_list_d2r_v3(&overflow, 8, &HuffmanTree::new(), 0x69);
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
    let result = Item::read_counted_list_d2r_v3(&wrong_mode, 8, &HuffmanTree::new(), 0x69);
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
    assert!(
        semantic_properties
            .iter()
            .all(|property| !property.is_opaque)
    );

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
