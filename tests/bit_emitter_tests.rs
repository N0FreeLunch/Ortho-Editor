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
