use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::domain::item::serialization::{
    write_property_list, BitEmitter, PropertyReaderContext,
};
use d2r_core::domain::stats::axiom::PropertyRhythm;
use d2r_core::domain::stats::{parse_single_property, ItemProperty, StatsAxiom};
use d2r_core::error::ParsingError;
use d2r_core::item::{HuffmanTree, ItemBitRange, ItemQuality};
use d2r_macros::{hypothesis_test, invariant_test};
use std::io::Cursor;

const CODES: [&str; 12] = [
    "plain",
    "xrs",
    "c8xr",
    "rhd",
    "wa2",
    " xrs ",
    "\u{2003}wa2\t",
    "Opaque",
    " Opaque ",
    "opaque",
    "",
    "xrsx",
];
const EXPLICIT_VERSION: u8 = 255;

// Frozen legacy selection, independent of the candidate data policy.
fn legacy(
    version: u8,
    alpha: bool,
    code: &str,
    runeword: bool,
    stat_id: u32,
) -> (u32, Option<u32>, bool, bool) {
    if !alpha || !matches!(version, 0 | 1 | 2 | 4 | 5 | 6 | 7) {
        return (9, None, true, false);
    }
    if stat_id == 320 {
        return (9, None, false, false);
    }
    if version == 0 && runeword && !matches!(code.trim(), "xrs" | "c8xr" | "rhd" | "wa2") {
        return (9, Some(7), false, false);
    }
    if runeword {
        return (9, Some(9), false, false);
    }
    if matches!(version, 0 | 1) {
        return (9, Some(8), false, false);
    }
    if matches!(version, 2 | 4 | 6) || code.trim() == "Opaque" {
        return (9, Some(9), false, false);
    }
    (9, None, false, false)
}

fn descriptor(rhythm: PropertyRhythm) -> (u32, Option<u32>, bool, bool) {
    (
        rhythm.id_bits,
        rhythm.value_bits,
        rhythm.has_terminal_bit,
        rhythm.has_extra_terminal_bit,
    )
}

fn axiom(version: u8, alpha: bool, code: &str) -> StatsAxiom {
    let mut result = StatsAxiom::new(version, ItemQuality::Normal, alpha);
    result.code = code.into();
    result
}

fn assert_stat97_witness_inputs() {
    let stat = d2r_core::data::stat_costs::STAT_COSTS
        .iter()
        .find(|stat| stat.id == 97)
        .unwrap();
    assert_eq!(
        (stat.save_bits, stat.save_param_bits, stat.save_add),
        (6, 9, 0)
    );
    let registry = get_registry();
    assert!(!registry.mappings.contains_key("97") && !registry.stats.contains_key("97"));
    for code in CODES {
        assert!(registry
            .item_overrides
            .as_ref()
            .and_then(|items| items.get(code.trim()))
            .and_then(|row| row.get("97"))
            .is_none());
    }
}

#[hypothesis_test]
fn rhythm_selector_preserves_versions_priority_and_ignored_inputs() {
    // The current registry has no mapped-to-320 witness; do not claim that arm was exercised.
    assert!(get_registry()
        .mappings
        .values()
        .all(|mapping| mapping.effective_id != 320));
    for (version, code, runeword, stat, expected) in [
        (0, "Opaque", true, 97, (9, Some(7), false, false)),
        (0, "xrs", true, 97, (9, Some(9), false, false)),
        (0, "Opaque", false, 97, (9, Some(8), false, false)),
        (5, "Opaque", false, 97, (9, Some(9), false, false)),
        (5, "plain", false, 97, (9, None, false, false)),
        (3, "Opaque", true, 97, (9, None, true, false)),
        (0, "Opaque", true, 320, (9, None, false, false)),
    ] {
        assert_eq!(
            descriptor(axiom(version, true, code).property_rhythm(runeword, false, false, stat)),
            expected
        );
    }
    let mut cases = 0;
    for version in u8::MIN..=u8::MAX {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for shadow in [false, true] {
                    for compact in [false, true] {
                        for code in CODES {
                            let mut a = axiom(version, alpha, code);
                            a.is_compact = !compact;
                            for stat in [0, 97, 320] {
                                assert_eq!(descriptor(a.property_rhythm(runeword, shadow, compact, stat)), legacy(version, alpha, code, runeword, stat), "version {version}, alpha {alpha}, code {code:?}, rw {runeword}, shadow {shadow}, compact {compact}, stat {stat}");
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 147_456);
}

fn wire(text: &str) -> Vec<bool> {
    text.bytes().map(|byte| byte == b'1').collect()
}

fn field(version: u8, alpha: bool, code: &str, runeword: bool) -> Vec<bool> {
    let width = legacy(version, alpha, code, runeword, 97).1.unwrap_or(6);
    // Stat 97, explicit version 255: parameter 341 is present and default rewrites are excluded.
    let mut bits = wire("1000011001010101011");
    bits.extend(std::iter::repeat_n(false, width as usize - 1));
    bits
}

fn packed(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .zip([1, 2, 4, 8, 16, 32, 64, 128])
                .filter_map(|(&bit, place)| bit.then_some(place))
                .sum()
        })
        .collect()
}

#[invariant_test]
fn writer_uses_rhythm_values_without_changing_following_bits() {
    assert_stat97_witness_inputs();
    let huffman = HuffmanTree::new();
    for version in u8::MIN..=u8::MAX {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for code in CODES {
                    let a = axiom(version, alpha, code);
                    let expected_field = field(version, alpha, code, runeword);
                    let property = ItemProperty::new(
                        97,
                        "rhythm witness".into(),
                        341,
                        1,
                        1,
                        ItemBitRange { start: 0, end: 0 },
                    );
                    let mut emitter = BitEmitter::new();
                    emitter.extend_bits(wire("101")).unwrap();
                    write_property_list(
                        &mut emitter,
                        code,
                        &[property],
                        &[],
                        &huffman,
                        EXPLICIT_VERSION,
                        runeword,
                        false,
                        false,
                        ItemQuality::Normal,
                        false,
                        &a,
                    )
                    .unwrap();
                    assert_eq!(emitter.written_bits(), 3 + expected_field.len() as u64);
                    emitter.extend_bits(wire("0110")).unwrap();
                    let expected: Vec<bool> = wire("101")
                        .into_iter()
                        .chain(expected_field)
                        .chain(wire("0110"))
                        .collect();
                    assert_eq!(
                        emitter.bits(),
                        expected,
                        "writer axiom {version}, alpha {alpha}, code {code:?}, rw {runeword}"
                    );
                    assert_eq!(emitter.into_bytes(), packed(&expected));
                }
            }
        }
    }
}

#[invariant_test]
fn reader_uses_rhythm_values_without_changing_ranges_or_guard() {
    assert_stat97_witness_inputs();
    let huffman = HuffmanTree::new();
    for version in u8::MIN..=u8::MAX {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for code in CODES {
                    let expected_field = field(version, alpha, code, runeword);
                    let bits: Vec<bool> = wire("101")
                        .into_iter()
                        .chain(expected_field.iter().copied())
                        .chain(wire("0110"))
                        .collect();
                    let bytes = packed(&bits);
                    let a = axiom(version, alpha, code);
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    cursor.read_bits::<u8>(3).unwrap();
                    let (property, terminal, terminal_bit, nested) = parse_single_property(
                        &mut cursor,
                        EXPLICIT_VERSION,
                        &huffman,
                        runeword,
                        &a,
                        PropertyReaderContext {
                            bytes: &bytes,
                            item_start_bit: 0,
                        },
                        |_, _, _, _, _| {
                            panic!("non-nested rhythm witness must not recover children")
                        },
                    )
                    .unwrap()
                    .unwrap();
                    assert_eq!(property.stat_id, 97);
                    assert_eq!(property.param, 341);
                    assert_eq!(
                        property.raw_value, 1,
                        "reader axiom {version}, alpha {alpha}, code {code:?}, rw {runeword}"
                    );
                    assert_eq!(property.value, 1);
                    assert_eq!(property.range.start, 3);
                    assert_eq!(property.range.end, 3 + expected_field.len() as u64);
                    assert_eq!(cursor.pos(), property.range.end);
                    assert!(!terminal && !terminal_bit && nested.is_empty() && !property.is_opaque);
                    assert_eq!(cursor.read_bits::<u8>(4).unwrap(), 6);
                    assert_eq!(
                        cursor
                            .recorded_bits()
                            .iter()
                            .map(|bit| bit.bit)
                            .collect::<Vec<_>>(),
                        bits
                    );
                }
            }
        }
    }
}

#[invariant_test]
fn incomplete_rhythm_value_preserves_error_recording_and_original_tail() {
    assert_stat97_witness_inputs();
    let huffman = HuffmanTree::new();
    for version in [0, 1, 2, 4, 5, 6, 7, 3, 255] {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for code in CODES {
                    let bits: Vec<bool> = wire("101")
                        .into_iter()
                        .chain(field(version, alpha, code, runeword))
                        .chain(wire("0110"))
                        .collect();
                    let bytes = packed(&bits);
                    let a = axiom(version, alpha, code);
                    let limit = bits.len() as u64 - 5;
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    cursor.read_bits::<u8>(3).unwrap();
                    cursor.set_limit(limit);
                    let error = parse_single_property(
                        &mut cursor,
                        EXPLICIT_VERSION,
                        &huffman,
                        runeword,
                        &a,
                        PropertyReaderContext {
                            bytes: &bytes,
                            item_start_bit: 0,
                        },
                        |_, _, _, _, _| panic!("no nested recovery"),
                    )
                    .unwrap_err();
                    assert_eq!(
                        error.error,
                        ParsingError::Io(
                            "Bit limit exceeded (possible section boundary violation)".into()
                        )
                    );
                    assert_eq!(error.bit_offset, limit);
                    assert_eq!(error.hint.as_deref(), Some("The parser tried to read beyond the allocated bits for this item or section."));
                    assert_eq!(cursor.pos(), limit);
                    assert_eq!(
                        cursor
                            .recorded_bits()
                            .iter()
                            .map(|bit| bit.bit)
                            .collect::<Vec<_>>(),
                        bits[..limit as usize]
                    );
                    cursor.set_limit(bits.len() as u64);
                    assert_eq!(cursor.read_bits::<u8>(5).unwrap(), 12);
                }
            }
        }
    }
}
