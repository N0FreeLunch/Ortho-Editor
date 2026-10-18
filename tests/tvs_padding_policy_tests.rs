use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::forensic::registry::get_registry;
use d2r_core::domain::item::serialization::{
    BitEmitter, PropertyReaderContext, write_property_list,
};
use d2r_core::domain::stats::{ItemProperty, StatsAxiom, parse_single_property};
use d2r_core::error::ParsingError;
use d2r_core::item::{HuffmanTree, ItemBitRange, ItemQuality};
use d2r_macros::{hypothesis_test, invariant_test};
use std::io::Cursor;

const CODES: [&str; 7] = [
    "plain",
    "xrs",
    "c8xr",
    "rhd",
    "wa2",
    " \txrs\u{2003}",
    "Opaque",
];
const PADDING: [bool; 9] = [true, false, true, true, false, false, true, false, true];
const GUARD: [bool; 11] = [
    false, true, false, true, true, false, true, false, true, false, true,
];

// Independent frozen legacy observations, not a lookup into the candidate policy.
fn legacy_tvs(version: u8, alpha: bool, runeword: bool, compact: bool) -> bool {
    alpha && matches!(version, 0 | 1 | 2 | 4 | 6) && !runeword && !compact
}

fn legacy_alpha(version: u8, alpha: bool) -> bool {
    alpha && matches!(version, 0 | 1 | 2 | 4 | 5 | 6 | 7)
}

fn authority(code: &str) -> bool {
    matches!(code.trim(), "xrs" | "c8xr" | "rhd" | "wa2")
}

fn axiom(version: u8, alpha: bool, compact: bool, code: &str) -> StatsAxiom {
    let mut a = StatsAxiom::new(version, ItemQuality::Normal, alpha);
    // Direct field assignment excludes with_code's independent compact inference.
    a.is_compact = compact;
    a.code = code.into();
    a
}

fn prefix(offset: usize) -> Vec<bool> {
    (0..offset).map(|i| i % 2 == 0).collect()
}

fn packed(bits: &[bool]) -> Vec<u8> {
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .fold(0, |byte, (i, &bit)| byte | (u8::from(bit) << i))
        })
        .collect()
}

fn stream(offset: usize, retail: bool, padding: bool) -> Vec<bool> {
    let mut bits = prefix(offset);
    bits.extend([true; 9]);
    if retail {
        bits.push(true);
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
    }
    if padding {
        bits.extend(PADDING);
    }
    bits
}

#[hypothesis_test]
fn tvs_selector_preserves_full_version_and_flag_domain() {
    let mut cases = 0;
    for version in u8::MIN..=u8::MAX {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for compact in [false, true] {
                    let mut a = axiom(version, alpha, compact, "unrelated");
                    a.quality = ItemQuality::Unique;
                    a.idx = usize::MAX;
                    a.is_socketed = true;
                    a.is_personalized = true;
                    assert_eq!(
                        a.has_tvs_padding(runeword),
                        legacy_tvs(version, alpha, runeword, compact),
                        "version {version}, alpha {alpha}, rw {runeword}, compact {compact}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 2_048);
}

#[invariant_test]
fn writer_preserves_tvs_completeness_capture_alignment_and_following_guard() {
    assert!(
        !d2r_core::data::stat_costs::STAT_COSTS
            .iter()
            .any(|stat| stat.id == 511)
    );
    assert!(!get_registry().mappings.contains_key("511"));
    let tree = HuffmanTree::new();
    let term = ItemProperty::new(
        511,
        "terminator witness".into(),
        0,
        0,
        0,
        ItemBitRange { start: 0, end: 0 },
    );
    let mut cases = 0;
    for version in u8::MIN..=u8::MAX {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for compact in [false, true] {
                    for complete in [false, true] {
                        for code in CODES {
                            for offset in [0, 3, 7] {
                                let a = axiom(version, alpha, compact, code);
                                let retail = !legacy_alpha(version, alpha);
                                let tvs = complete && legacy_tvs(version, alpha, runeword, compact);
                                let mut wire = prefix(offset);
                                wire.extend([true; 9]);
                                let mut capture = wire.clone();
                                if complete && retail {
                                    wire.push(true);
                                    capture.push(true);
                                    while wire.len() % 8 != 0 {
                                        wire.push(false);
                                    }
                                }
                                if tvs {
                                    wire.extend([false; 9]);
                                    capture.extend([false; 9]);
                                }
                                let mut emitter = BitEmitter::new();
                                emitter.extend_bits(prefix(offset)).unwrap();
                                // Explicit code/version differ from the receiver; TVS belongs to the receiver.
                                write_property_list(
                                    &mut emitter,
                                    "plain",
                                    std::slice::from_ref(&term),
                                    &[],
                                    &tree,
                                    255,
                                    runeword,
                                    true,
                                    complete,
                                    ItemQuality::Normal,
                                    false,
                                    &a,
                                )
                                .unwrap();
                                assert_eq!(emitter.written_bits(), wire.len() as u64);
                                emitter.extend_bits(GUARD.to_vec()).unwrap();
                                wire.extend(GUARD);
                                capture.extend(GUARD);
                                assert_eq!(
                                    emitter.bits(),
                                    capture,
                                    "version {version}, alpha {alpha}, rw {runeword}, compact {compact}, complete {complete}, code {code:?}, offset {offset}"
                                );
                                assert_eq!(emitter.into_bytes(), packed(&wire));
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 86_016);
}

#[invariant_test]
fn reader_preserves_terminal_ranges_nonzero_padding_and_following_guard() {
    let tree = HuffmanTree::new();
    let mut cases = 0;
    for version in u8::MIN..=u8::MAX {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for compact in [false, true] {
                    for code in CODES {
                        for offset in [0, 3, 7] {
                            for bounded in [false, true] {
                                let retail = !legacy_alpha(version, alpha);
                                let mut bits = stream(
                                    offset,
                                    retail,
                                    legacy_tvs(version, alpha, runeword, compact),
                                );
                                let end = bits.len() as u64;
                                bits.extend(GUARD);
                                let bytes = packed(&bits);
                                let a = axiom(version, alpha, compact, code);
                                let mut cursor = BitCursor::new(BitReader::endian(
                                    Cursor::new(&bytes),
                                    LittleEndian,
                                ));
                                for bit in prefix(offset) {
                                    assert_eq!(cursor.read_bit().unwrap(), bit);
                                }
                                if bounded {
                                    cursor.set_limit(bits.len() as u64);
                                }
                                let (prop, terminal, terminal_bit, nested) = parse_single_property(
                                    &mut cursor,
                                    255,
                                    &tree,
                                    runeword,
                                    &a,
                                    PropertyReaderContext {
                                        bytes: &bytes,
                                        item_start_bit: 0,
                                    },
                                    |_, _, _, _, _| panic!("terminator must not recover children"),
                                )
                                .unwrap()
                                .unwrap();
                                assert_eq!(
                                    (prop.stat_id, prop.param, prop.raw_value, prop.value),
                                    (511, 0, 0, 0)
                                );
                                assert_eq!(prop.name, "terminator");
                                assert_eq!(
                                    (prop.range.start, prop.range.end),
                                    (offset as u64, end)
                                );
                                assert!(
                                    terminal
                                        && nested.is_empty()
                                        && !prop.is_opaque
                                        && prop.opaque_bits.is_none()
                                );
                                assert_eq!(terminal_bit, retail);
                                assert_eq!(cursor.pos(), end);
                                for bit in GUARD {
                                    assert_eq!(cursor.read_bit().unwrap(), bit);
                                }
                                assert_eq!(
                                    cursor
                                        .recorded_bits()
                                        .iter()
                                        .map(|bit| bit.bit)
                                        .collect::<Vec<_>>(),
                                    bits
                                );
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 86_016);
}

#[invariant_test]
fn short_tvs_preserves_bounded_host_skip_partial_errors_and_unbounded_eof() {
    let tree = HuffmanTree::new();
    let mut bounded_cases = 0;
    let mut eof_cases = 0;
    for version in [0, 1, 2, 4, 6] {
        for code in CODES {
            for offset in 0..8 {
                for available in 0..9 {
                    let mut bits = stream(offset, false, true);
                    bits.extend(GUARD);
                    let bytes = packed(&bits);
                    let a = axiom(version, true, false, code);
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    for bit in prefix(offset) {
                        assert_eq!(cursor.read_bit().unwrap(), bit);
                    }
                    let term_end = offset as u64 + 9;
                    let limit = term_end + available;
                    cursor.set_limit(limit);
                    let result = parse_single_property(
                        &mut cursor,
                        255,
                        &tree,
                        false,
                        &a,
                        PropertyReaderContext {
                            bytes: &bytes,
                            item_start_bit: 0,
                        },
                        |_, _, _, _, _| panic!("no nested recovery"),
                    );
                    if authority(code) {
                        let (prop, terminal, terminal_bit, nested) = result.unwrap().unwrap();
                        assert!(terminal && !terminal_bit && nested.is_empty());
                        assert_eq!(
                            (prop.range.start, prop.range.end),
                            (offset as u64, term_end)
                        );
                        assert_eq!(cursor.pos(), term_end);
                    } else {
                        let error = result.unwrap_err();
                        assert_eq!(
                            error.error,
                            ParsingError::Io(
                                "Bit limit exceeded (possible section boundary violation)".into()
                            )
                        );
                        assert_eq!(error.bit_offset, limit);
                        assert_eq!(
                            error.hint.as_deref(),
                            Some(
                                "The parser tried to read beyond the allocated bits for this item or section."
                            )
                        );
                        assert_eq!(cursor.pos(), limit);
                    }
                    let consumed = cursor.pos() as usize;
                    assert_eq!(
                        cursor
                            .recorded_bits()
                            .iter()
                            .map(|bit| bit.bit)
                            .collect::<Vec<_>>(),
                        bits[..consumed]
                    );
                    cursor.set_limit(bits.len() as u64);
                    for bit in &bits[consumed..] {
                        assert_eq!(cursor.read_bit().unwrap(), *bit);
                    }
                    assert_eq!(
                        cursor
                            .recorded_bits()
                            .iter()
                            .map(|bit| bit.bit)
                            .collect::<Vec<_>>(),
                        bits
                    );
                    bounded_cases += 1;
                }
            }
            // Exactly two bytes: unbounded remaining() cannot detect physical EOF.
            let bits = stream(7, false, false);
            assert_eq!(bits.len(), 16);
            let bytes = packed(&bits);
            let a = axiom(version, true, false, code);
            let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
            for bit in prefix(7) {
                assert_eq!(cursor.read_bit().unwrap(), bit);
            }
            let error = parse_single_property(
                &mut cursor,
                255,
                &tree,
                false,
                &a,
                PropertyReaderContext {
                    bytes: &bytes,
                    item_start_bit: 0,
                },
                |_, _, _, _, _| panic!("no nested recovery"),
            )
            .unwrap_err();
            assert!(
                matches!(error.error, ParsingError::Io(ref message) if message.starts_with("Bit-level read failure: "))
            );
            assert_eq!(error.bit_offset, 16);
            assert_eq!(
                error.hint.as_deref(),
                Some("Possible end of bitstream reached unexpectedly.")
            );
            assert_eq!(cursor.pos(), 16);
            assert_eq!(
                cursor
                    .recorded_bits()
                    .iter()
                    .map(|bit| bit.bit)
                    .collect::<Vec<_>>(),
                bits
            );
            eof_cases += 1;
        }
    }
    assert_eq!(bounded_cases, 2_520);
    assert_eq!(eof_cases, 35);
}
