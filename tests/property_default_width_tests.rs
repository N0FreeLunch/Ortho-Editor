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

#[derive(Clone, Copy)]
struct Context {
    axiom_version: u8,
    code: &'static str,
    stat_id: u32,
}

const CONTEXTS: [Context; 12] = [
    Context {
        axiom_version: 3,
        code: "plain",
        stat_id: 0,
    },
    Context {
        axiom_version: 3,
        code: "plain",
        stat_id: 509,
    },
    Context {
        axiom_version: 5,
        code: "plain",
        stat_id: 509,
    },
    Context {
        axiom_version: 1,
        code: "plain",
        stat_id: 0,
    },
    Context {
        axiom_version: 0,
        code: "plain",
        stat_id: 0,
    },
    Context {
        axiom_version: 5,
        code: "wsp",
        stat_id: 31,
    },
    Context {
        axiom_version: 5,
        code: "plain",
        stat_id: 31,
    },
    Context {
        axiom_version: 5,
        code: "plain",
        stat_id: 152,
    },
    Context {
        axiom_version: 5,
        code: "plain",
        stat_id: 8,
    },
    Context {
        axiom_version: 5,
        code: "acww",
        stat_id: 8,
    },
    Context {
        axiom_version: 5,
        code: "wsp",
        stat_id: 8,
    },
    Context {
        axiom_version: 0,
        code: "plain",
        stat_id: 509,
    },
];

fn axiom(context: Context, alpha: bool) -> StatsAxiom {
    let mut result = StatsAxiom::new(context.axiom_version, ItemQuality::Normal, alpha);
    result.code = context.code.into();
    result
}

// Frozen source/registry oracle for the selected contexts, independent of the candidate helper/data.
fn legacy_width(context: Context, alpha: bool, version: u8, runeword: bool) -> u32 {
    let known = match context.stat_id {
        0 => Some(8),
        8 => Some(0),
        31 => Some(11),
        152 => Some(1),
        509 => None,
        _ => unreachable!(),
    };
    let is_alpha = alpha && matches!(context.axiom_version, 0 | 1 | 2 | 4 | 5 | 6 | 7);
    let rhythm = if !is_alpha {
        None
    } else if runeword {
        Some(if context.axiom_version == 0 { 7 } else { 9 })
    } else if matches!(context.axiom_version, 0 | 1) {
        Some(8)
    } else if matches!(context.axiom_version, 2 | 4 | 6) {
        Some(9)
    } else {
        None
    };
    let mut width = known
        .map(|save_bits| rhythm.unwrap_or(save_bits))
        .unwrap_or(9);
    if alpha && matches!(version, 0 | 1) && width == 9 {
        width = 8;
    }
    if is_alpha {
        if context.code == "wsp" && context.stat_id == 31 {
            return 11;
        }
        if context.code == "wsp" && context.stat_id == 8 {
            return 0;
        }
        if width == 9 && context.code != "acww" {
            return 9;
        }
        if context.stat_id == 152 {
            return 1;
        }
        if matches!(context.stat_id, 8 | 31) {
            return 10;
        }
    }
    width
}

fn wire(text: &str) -> Vec<bool> {
    text.bytes().map(|byte| byte == b'1').collect()
}

fn field(stat_id: u32, width: u32) -> Vec<bool> {
    let mut bits = wire(match stat_id {
        0 => "000000000",
        8 => "000100000",
        31 => "111110000",
        152 => "000110010",
        509 => "101111111",
        _ => unreachable!(),
    });
    if width > 0 {
        bits.push(true);
        bits.extend(std::iter::repeat_n(false, width as usize - 1));
    }
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

fn check_writer(huffman: &HuffmanTree, context: Context, alpha: bool, version: u8, runeword: bool) {
    let width = legacy_width(context, alpha, version, runeword);
    let expected_field = field(context.stat_id, width);
    let a = axiom(context, alpha);
    let property = ItemProperty::new(
        context.stat_id,
        "width witness".into(),
        0,
        1,
        1,
        ItemBitRange { start: 0, end: 0 },
    );
    let mut emitter = BitEmitter::new();
    emitter.extend_bits(wire("101")).unwrap();
    write_property_list(
        &mut emitter,
        context.code,
        &[property],
        &[],
        huffman,
        version,
        runeword,
        false,
        false,
        ItemQuality::Normal,
        false,
        &a,
    )
    .unwrap();
    assert_eq!(
        emitter.written_bits(),
        3 + expected_field.len() as u64,
        "writer stat {}, code {}, axiom {}, explicit {version}, alpha {alpha}, rw {runeword}",
        context.stat_id,
        context.code,
        context.axiom_version
    );
    emitter.extend_bits(wire("0110")).unwrap();
    let expected: Vec<bool> = wire("101")
        .into_iter()
        .chain(expected_field)
        .chain(wire("0110"))
        .collect();
    assert_eq!(emitter.bits(), expected);
    assert_eq!(emitter.into_bytes(), packed(&expected));
}

fn check_reader(huffman: &HuffmanTree, context: Context, alpha: bool, version: u8, runeword: bool) {
    let width = legacy_width(context, alpha, version, runeword);
    let expected_field = field(context.stat_id, width);
    let bits: Vec<bool> = wire("101")
        .into_iter()
        .chain(expected_field.iter().copied())
        .chain(wire("0110"))
        .collect();
    let bytes = packed(&bits);
    let a = axiom(context, alpha);
    let mut cursor = BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
    cursor.read_bits::<u8>(3).unwrap();
    let (property, terminal, terminal_bit, nested) = parse_single_property(
        &mut cursor,
        version,
        huffman,
        runeword,
        &a,
        PropertyReaderContext {
            bytes: &bytes,
            item_start_bit: 0,
        },
        |_, _, _, _, _| panic!("non-nested width witness must not recover children"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(property.stat_id, context.stat_id);
    assert_eq!(property.param, 0);
    assert_eq!(
        property.raw_value,
        i32::from(width > 0),
        "reader stat {}, code {}, axiom {}, explicit {version}, alpha {alpha}, rw {runeword}",
        context.stat_id,
        context.code,
        context.axiom_version
    );
    assert_eq!(property.range.start, 3);
    assert_eq!(property.range.end, 3 + expected_field.len() as u64);
    assert_eq!(cursor.pos(), property.range.end);
    assert_eq!(property.is_opaque, context.stat_id == 509);
    assert!(!terminal && !terminal_bit && nested.is_empty());
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

#[invariant_test]
fn writer_default_width_and_override_precedence_preserve_all_versions() {
    let huffman = HuffmanTree::new();
    for alpha in [false, true] {
        for runeword in [false, true] {
            for version in u8::MIN..=u8::MAX {
                for context in CONTEXTS {
                    check_writer(&huffman, context, alpha, version, runeword);
                }
            }
        }
    }
}

#[invariant_test]
fn reader_default_width_and_override_precedence_preserve_all_versions() {
    let huffman = HuffmanTree::new();
    for alpha in [false, true] {
        for runeword in [false, true] {
            for version in u8::MIN..=u8::MAX {
                for context in CONTEXTS {
                    check_reader(&huffman, context, alpha, version, runeword);
                }
            }
        }
    }
}

#[invariant_test]
fn pinned_width_canaries_distinguish_fallback_rewrite_and_registry_order() {
    let huffman = HuffmanTree::new();
    let cases = [
        (5, "wsp", 31, true, 5, true, 11),
        (5, "wsp", 8, true, 5, true, 0),
        (5, "plain", 152, true, 5, true, 9),
        (5, "plain", 152, true, 0, true, 1),
        (5, "acww", 8, true, 5, true, 10),
        (5, "plain", 8, true, 5, true, 9),
        (5, "plain", 8, true, 0, true, 10),
        (0, "plain", 509, true, 2, true, 9),
        (0, "plain", 509, true, 0, true, 8),
        (3, "plain", 509, true, 0, false, 8),
        (0, "plain", 509, false, 0, true, 9),
    ];
    for (axiom_version, code, stat_id, alpha, version, runeword, expected) in cases {
        let context = Context {
            axiom_version,
            code,
            stat_id,
        };
        assert_eq!(legacy_width(context, alpha, version, runeword), expected);
        check_writer(&huffman, context, alpha, version, runeword);
        check_reader(&huffman, context, alpha, version, runeword);
    }
}

#[invariant_test]
fn incomplete_width_fields_preserve_error_prefix_and_remaining_guard() {
    let huffman = HuffmanTree::new();
    for context in CONTEXTS {
        for alpha in [false, true] {
            for runeword in [false, true] {
                for version in [0, 1, 2, 255] {
                    let expected_field = field(
                        context.stat_id,
                        legacy_width(context, alpha, version, runeword),
                    );
                    let tail = u8::from(*expected_field.last().unwrap()) | 12;
                    let bits: Vec<bool> = wire("101")
                        .into_iter()
                        .chain(expected_field)
                        .chain(wire("0110"))
                        .collect();
                    let bytes = packed(&bits);
                    let limit = bits.len() as u64 - 5;
                    let a = axiom(context, alpha);
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
                            .map(|bit| bit.bit)
                            .collect::<Vec<_>>(),
                        bits[..limit as usize]
                    );
                    cursor.set_limit(bits.len() as u64);
                    assert_eq!(cursor.read_bits::<u8>(5).unwrap(), tail);
                }
            }
        }
    }
}
