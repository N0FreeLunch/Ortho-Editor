use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::item::serialization::{
    write_property_list, BitEmitter, PropertyReaderContext,
};
use d2r_core::domain::stats::{parse_single_property, ItemProperty, StatsAxiom};
use d2r_core::error::ParsingError;
use d2r_core::item::{HuffmanTree, ItemBitRange, ItemQuality};
use d2r_macros::invariant_test;
use std::io::Cursor;

const CODES: [&str; 14] = [
    "xrs",
    "c8xr",
    "rhd",
    "wa2",
    " xrs ",
    "\txrs\n",
    "\u{2003}rhd\t",
    "XRS",
    "xrs\0",
    "x rs",
    "",
    "unknown",
    "scs",
    "rhdx",
];

// Freeze the old predicate without reading the candidate policy table.
fn legacy_omits(alpha: bool, code: &str, version: u8, runeword: bool) -> bool {
    let authority = alpha && matches!(code.trim(), "xrs" | "c8xr" | "rhd" | "wa2");
    alpha
        && (runeword || authority)
        && matches!(code.trim(), "xrs" | "c8xr" | "rhd")
        && (version == 1 || version == 0)
}

fn wire(text: &str) -> Vec<bool> {
    text.bytes().map(|byte| byte == b'1').collect()
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

fn field(omits: bool, alpha_value: bool) -> Vec<bool> {
    // Stat 97, parameter 341, raw value 45; independent little-endian wire.
    let mut result = wire("100001100");
    if !omits {
        result.extend(wire("101010101"));
    }
    result.extend(wire(if alpha_value { "10110100" } else { "101101" }));
    result
}

fn axiom(version: u8, alpha: bool, code: &str) -> StatsAxiom {
    let mut result = StatsAxiom::new(version, ItemQuality::Normal, alpha);
    result.code = code.to_owned();
    result
}

fn check_writer(
    huffman: &HuffmanTree,
    a: &StatsAxiom,
    code: &str,
    version: u8,
    runeword: bool,
    alpha_value: bool,
) {
    let expected_field = field(
        legacy_omits(a.save_is_alpha, code, version, runeword),
        alpha_value,
    );
    let property = ItemProperty::new(
        97,
        "wire witness".into(),
        341,
        45,
        45,
        ItemBitRange { start: 0, end: 0 },
    );
    let mut emitter = BitEmitter::new();
    emitter.extend_bits(wire("101")).unwrap();
    write_property_list(
        &mut emitter,
        code,
        &[property],
        &[],
        huffman,
        version,
        runeword,
        false,
        false,
        ItemQuality::Normal,
        false,
        a,
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
        "writer code {code:?}, version {version}, alpha {}, rw {runeword}",
        a.save_is_alpha
    );
    assert_eq!(emitter.into_bytes(), packed(&expected));
}

fn check_reader(
    huffman: &HuffmanTree,
    a: &StatsAxiom,
    version: u8,
    runeword: bool,
    alpha_value: bool,
) {
    let omits = legacy_omits(a.save_is_alpha, &a.code, version, runeword);
    let expected_field = field(omits, alpha_value);
    let expected: Vec<bool> = wire("101")
        .into_iter()
        .chain(expected_field.iter().copied())
        .chain(wire("0110"))
        .collect();
    let bytes = packed(&expected);
    let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
    assert_eq!(cursor.read_bits::<u8>(3).unwrap(), 5);
    let (property, terminal, terminal_bit, nested) = parse_single_property(
        &mut cursor,
        version,
        huffman,
        runeword,
        a,
        PropertyReaderContext {
            bytes: &bytes,
            item_start_bit: 0,
        },
        |_, _, _, _, _| panic!("non-nested witness must not invoke recovery"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(property.stat_id, 97);
    assert_eq!(
        property.param,
        if omits { 0 } else { 341 },
        "reader code {:?}, version {version}, alpha {}, rw {runeword}",
        a.code,
        a.save_is_alpha
    );
    assert_eq!(property.raw_value, 45);
    assert_eq!(property.value, 45);
    assert_eq!(property.range.start, 3);
    assert_eq!(property.range.end, 3 + expected_field.len() as u64);
    assert!(!terminal && !terminal_bit && nested.is_empty() && !property.is_opaque);
    assert_eq!(cursor.pos(), property.range.end);
    assert_eq!(cursor.read_bits::<u8>(4).unwrap(), 6);
    assert_eq!(
        cursor
            .recorded_bits()
            .iter()
            .map(|b| b.bit)
            .collect::<Vec<_>>(),
        expected
    );
}

#[invariant_test]
fn writer_parameter_admission_preserves_all_explicit_versions() {
    let huffman = HuffmanTree::new();
    for alpha in [false, true] {
        // Version 3 is intentionally outside is_alpha; code differs from the argument.
        let a = axiom(3, alpha, "different-axiom-code");
        for runeword in [false, true] {
            for version in u8::MIN..=u8::MAX {
                for code in CODES {
                    check_writer(&huffman, &a, code, version, runeword, false);
                }
            }
        }
    }
}

#[invariant_test]
fn reader_parameter_admission_preserves_all_explicit_versions() {
    let huffman = HuffmanTree::new();
    for alpha in [false, true] {
        for runeword in [false, true] {
            for version in u8::MIN..=u8::MAX {
                for code in CODES {
                    check_reader(&huffman, &axiom(3, alpha, code), version, runeword, false);
                }
            }
        }
    }
}

#[invariant_test]
fn recognized_alpha_context_keeps_explicit_code_and_version_authority() {
    let huffman = HuffmanTree::new();
    for version in [0, 1, 2, 255] {
        for code in ["xrs", "wa2", "\u{2003}rhd\t"] {
            check_reader(&huffman, &axiom(1, true, code), version, false, true);
            let different = if code.trim() == "wa2" { "xrs" } else { "wa2" };
            check_writer(
                &huffman,
                &axiom(1, true, different),
                code,
                version,
                false,
                true,
            );
        }
    }
}

#[invariant_test]
fn incomplete_parameter_value_preserves_error_recorded_prefix_and_remaining_bits() {
    let huffman = HuffmanTree::new();
    for alpha in [false, true] {
        for runeword in [false, true] {
            for version in [0, 1, 2, 255] {
                for code in ["xrs", "wa2", " xrs "] {
                    let a = axiom(3, alpha, code);
                    let bits: Vec<bool> = wire("101")
                        .into_iter()
                        .chain(field(legacy_omits(alpha, code, version, runeword), false))
                        .chain(wire("0110"))
                        .collect();
                    let bytes = packed(&bits);
                    let limit = bits.len() as u64 - 5;
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    cursor.read_bits::<u8>(3).unwrap();
                    cursor.set_limit(limit);
                    let error = parse_single_property(
                        &mut cursor,
                        version,
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
                            .map(|b| b.bit)
                            .collect::<Vec<_>>(),
                        bits[..limit as usize]
                    );
                    cursor.set_limit(bits.len() as u64);
                    assert_eq!(cursor.read_bits::<u8>(5).unwrap(), 13);
                }
            }
        }
    }
}
