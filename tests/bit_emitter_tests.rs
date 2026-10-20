use d2r_core::domain::item::serialization::{write_player_name, BitEmitter};
use d2r_macros::invariant_test;

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

#[invariant_test]
fn integer_widths_preserve_frozen_wire_and_following_bits() {
    let cases = [
        (0xfedc_ba98_u64, "00011001010111010011101101111111", false),
        (
            0xfedc_ba98_7654_3210_u64,
            concat!(
                "00001000", "01001100", "00101010", "01101110", "00011001", "01011101", "00111011",
                "01111111"
            ),
            true,
        ),
    ];
    for (value, frozen, wide) in cases {
        for count in 0..=80_u32 {
            let field: Vec<bool> = wire(frozen)
                .into_iter()
                .chain(std::iter::repeat(false))
                .take(count as usize)
                .collect();
            let expected: Vec<bool> = wire("101")
                .into_iter()
                .chain(field)
                .chain(wire("01101"))
                .collect();
            let emit = || {
                let mut emitter = BitEmitter::new();
                emitter.extend_bits(wire("101")).unwrap();
                if wide {
                    emitter.write_bits_u64(value, count).unwrap();
                } else {
                    emitter.write_bits(value as u32, count).unwrap();
                }
                for bit in wire("01101") {
                    emitter.write_bit(bit).unwrap();
                }
                emitter
            };
            let emitter = emit();
            assert_eq!(emitter.written_bits(), u64::from(count) + 8);
            assert_eq!(emitter.bits(), expected, "width {count}, wide {wide}");
            assert_eq!(emitter.into_bytes(), packed(&expected));
            assert_eq!(emit().into_bits(), expected);
        }
    }
}

#[invariant_test]
fn explicit_alignment_preserves_capture_and_subsequent_byte_position() {
    let partial_bytes = [0_u8, 1, 3, 7, 15, 31, 63, 127];
    for prefix in 0..=16_usize {
        let emit = || {
            let mut emitter = BitEmitter::new();
            emitter.extend_bits(vec![true; prefix]).unwrap();
            emitter.byte_align().unwrap();
            let aligned = prefix.div_ceil(8) * 8;
            assert_eq!(emitter.written_bits(), aligned as u64);
            assert_eq!(emitter.bits(), vec![true; prefix]);
            emitter.byte_align().unwrap();
            assert_eq!(emitter.written_bits(), aligned as u64);
            emitter.extend_bits(wire("1011")).unwrap();
            assert_eq!(emitter.written_bits(), aligned as u64 + 4);
            emitter.byte_align().unwrap();
            assert_eq!(emitter.written_bits(), aligned as u64 + 8);
            emitter.byte_align().unwrap();
            emitter
        };
        let expected_capture: Vec<bool> =
            vec![true; prefix].into_iter().chain(wire("1011")).collect();
        let mut expected_bytes = vec![255; prefix / 8];
        if prefix % 8 != 0 {
            expected_bytes.push(partial_bytes[prefix % 8]);
        }
        expected_bytes.push(13);
        let emitter = emit();
        assert_eq!(emitter.bits(), expected_capture);
        assert_eq!(emitter.into_bytes(), expected_bytes);
        assert_eq!(emit().into_bits(), expected_capture);
    }
}

#[invariant_test]
fn empty_zero_width_and_public_aliases_keep_one_output_type() {
    fn via_item(emitter: d2r_core::domain::item::BitEmitter) -> BitEmitter {
        emitter
    }
    let mut emitter = via_item(BitEmitter::new());
    emitter.write_bits(u32::MAX, 0).unwrap();
    emitter.write_bits_u64(u64::MAX, 0).unwrap();
    emitter.extend_bits(std::iter::empty()).unwrap();
    emitter.byte_align().unwrap();
    assert_eq!(emitter.written_bits(), 0);
    assert!(emitter.bits().is_empty());
    assert!(emitter.into_bytes().is_empty());
    assert!(BitEmitter::new().into_bits().is_empty());
}

#[invariant_test]
fn player_name_consumer_preserves_seven_bit_wire_with_prefix() {
    // Three prefix bits, 'A' (65), 'z' (122), and the zero name terminator.
    let expected = wire(concat!("101", "1000001", "0101111", "0000000"));
    for alpha in [false, true] {
        let mut emitter = BitEmitter::new();
        emitter.extend_bits(wire("101")).unwrap();
        write_player_name(&mut emitter, "Az", alpha).unwrap();
        assert_eq!(emitter.written_bits(), 24);
        assert_eq!(emitter.bits(), expected);
        assert_eq!(emitter.into_bytes(), [13, 234, 1]);
    }
}

#[d2r_macros::hypothesis_test]
fn player_name_payload_observations() {
    use bitstream_io::{BitRead, BitReader, LittleEndian};
    use d2r_core::data::bit_cursor::BitCursor;
    use d2r_core::domain::item::serialization::read_player_name;
    use std::io::Cursor;

    fn state<R: BitRead>(cursor: &BitCursor<R>) -> String {
        format!(
            "{:?}",
            (
                cursor.pos(),
                cursor.limit(),
                cursor.remaining(),
                cursor.recorded_bits(),
                cursor.segments(),
                cursor.active_segments(),
                cursor.context_stack(),
                cursor.operation_events(),
                cursor.alpha_quality
            )
        )
    }

    let names = [
        "",
        "Az",
        "A\0Z",
        "\0",
        "aaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbb",
        "cccccccccccccccc",
        "\u{00e9}",
        "\u{0080}",
        "\u{1f63a}",
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "a\u{0001}z",
    ];
    assert_eq!(names.len(), 12);
    let mut writers = Vec::new();
    let mut readers = Vec::new();
    let mut successes = 0;
    let mut errors = 0;
    for name in names {
        for prefix in 0..8_usize {
            for alpha in [false, true] {
                let mut emitter = BitEmitter::new();
                emitter
                    .extend_bits((0..prefix).map(|i| i % 2 == 0))
                    .unwrap();
                write_player_name(&mut emitter, name, alpha).unwrap();
                let name_end = emitter.written_bits();
                emitter.extend_bits(wire("11010")).unwrap();
                let written = emitter.written_bits();
                let capture = emitter.bits().to_vec();
                let bytes = emitter.into_bytes();
                writers.push(serde_json::json!({"name": name, "prefix": prefix, "alpha": alpha, "name_end": name_end, "written": written, "capture": capture, "bytes": bytes}));
                for limit in [
                    None,
                    Some(0),
                    Some(1),
                    Some(6),
                    Some(7),
                    Some(13),
                    Some(14),
                    Some(104),
                    Some(105),
                    Some(106),
                ] {
                    let mut cursor =
                        BitCursor::new(BitReader::endian(Cursor::new(&bytes), LittleEndian));
                    cursor.set_trace(true);
                    cursor.push_context("player name payload witness");
                    cursor.read_bits_as_vec(prefix as u32).unwrap();
                    if let Some(limit) = limit {
                        cursor.set_limit(prefix as u64 + limit);
                    }
                    let result = read_player_name(&mut cursor, alpha);
                    if result.is_ok() {
                        successes += 1;
                    } else {
                        errors += 1;
                    }
                    let after_name = state(&cursor);
                    let following = if result.is_ok() {
                        Some(format!("{:?}", cursor.read_bits::<u8>(5)))
                    } else {
                        None
                    };
                    readers.push(serde_json::json!({"name": name, "prefix": prefix, "alpha": alpha, "limit": limit, "result": format!("{result:?}"), "after_name": after_name, "following": following, "final_cursor": state(&cursor)}));
                }
            }
        }
    }
    assert_eq!(writers.len(), 192);
    assert_eq!(readers.len(), 1920);
    assert_eq!(successes + errors, 1920);
    assert!(successes > 0 && errors > 0);
    if let Some(path) = std::env::var_os("D2R_PLAYER_NAME_CODEC_OUTPUT") {
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer_pretty(output, &serde_json::json!({"schema": 1, "name_cases": 12, "writer_cases": writers.len(), "reader_cases": readers.len(), "successes": successes, "errors": errors, "writers": writers, "readers": readers})).unwrap();
    }
}
