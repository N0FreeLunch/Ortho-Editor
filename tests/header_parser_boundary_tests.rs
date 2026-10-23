use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::header::{parse_header, HeaderAxiom};
use d2r_core::domain::payload::HuffmanTree;
use d2r_core::error::ParsingError;
use d2r_macros::hypothesis_test;
use std::io::Cursor;

fn frame(flags: u32, prefix: usize) -> (Vec<u8>, u64) {
    let mut bits = vec![true; prefix];
    for (value, width) in [(flags, 32), (3, 3), (5, 3), (6, 3), (9, 4), (10, 4), (7, 3)] {
        bits.extend((0..width).map(|i| (value >> i) & 1 != 0));
    }
    if flags & (1 << 11) != 0 {
        bits.extend([true, false, true]);
    }
    let width = bits.len() as u64 - prefix as u64;
    bits.extend([true, false]);
    let mut bytes = vec![0; bits.len().div_ceil(8)];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 1 << (index % 8);
        }
    }
    (bytes, width)
}

fn cursor(
    bytes: &[u8],
    start: u64,
    limit: u64,
) -> BitCursor<BitReader<Cursor<&[u8]>, LittleEndian>> {
    let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
    reader.skip(start as u32).unwrap();
    let mut cursor = BitCursor::new(reader);
    cursor.set_pos(start);
    cursor.set_limit(limit);
    cursor.set_trace(true);
    cursor.push_context("Root");
    cursor
}

// These are preservation hypotheses for the legacy API, not new format authority.
#[hypothesis_test]
fn nonalpha_fields_and_following_bits_preserve_legacy_contract() {
    let huffman = HuffmanTree::new();
    for flags in [0, u32::MAX, 1 << 11, (1 << 4) | (1 << 21) | (1 << 26)] {
        for start in [0, 3] {
            let (bytes, width) = frame(flags, start);
            let mut cursor = cursor(&bytes, start as u64, start as u64 + width + 2);
            let header =
                parse_header(&mut cursor, &HeaderAxiom::new(255, false), &huffman, None).unwrap();
            assert_eq!(
                (
                    header.flags,
                    header.version,
                    header.mode,
                    header.location,
                    header.x,
                    header.y,
                    header.page
                ),
                (flags, 3, 5, 6, 9, 10, 7)
            );
            assert_eq!(
                header.socket_hint,
                if flags & (1 << 11) != 0 { 5 } else { 0 }
            );
            assert_eq!(header.is_compact, flags & (1 << 21) != 0);
            assert_eq!(header.is_personalized, flags & (1 << 24) != 0);
            assert_eq!(header.is_socketed, flags & (1 << 11) != 0);
            assert!(!header.save_is_alpha);
            assert_eq!(cursor.pos(), start as u64 + width);
            assert_eq!(cursor.context_stack(), ["Root"]);
            assert_eq!(cursor.segments().last().unwrap().label, "Header");
            assert!(cursor.read_bit().unwrap());
        }
    }
}

#[hypothesis_test]
fn truncated_nonalpha_retains_partial_bits_and_open_header_context() {
    let huffman = HuffmanTree::new();
    for flags in [0, u32::MAX] {
        let (bytes, width) = frame(flags, 3);
        for cut in 0..width {
            let mut cursor = cursor(&bytes, 3, 3 + cut);
            let failure =
                parse_header(&mut cursor, &HeaderAxiom::new(5, false), &huffman, None).unwrap_err();
            assert!(
                matches!(failure.error, ParsingError::Io(ref message) if message == "Bit limit exceeded (possible section boundary violation)")
            );
            assert_eq!(failure.bit_offset, 3 + cut);
            assert_eq!(
                failure.hint.as_deref(),
                Some(
                    "The parser tried to read beyond the allocated bits for this item or section."
                )
            );
            assert_eq!(failure.context_stack, ["Root", "Header"]);
            assert_eq!(failure.context_relative_offset, cut);
            assert_eq!(cursor.context_stack(), ["Root", "Header"]);
            assert_eq!(cursor.pos(), 3 + cut);
            assert_eq!(cursor.recorded_bits().len(), cut as usize);
            assert!(cursor.segments().is_empty());
        }
    }
}

#[hypothesis_test]
fn alpha_missing_context_preserves_entry_context_and_error() {
    let huffman = HuffmanTree::new();
    let mut cursor = cursor(&[], 0, 0);
    let failure =
        parse_header(&mut cursor, &HeaderAxiom::new(5, true), &huffman, None).unwrap_err();
    assert!(
        matches!(failure.error, ParsingError::Generic(ref message) if message == "Alpha v105 requires context for heuristic sync")
    );
    assert_eq!(failure.context_stack, ["Root", "Header"]);
    assert_eq!(cursor.context_stack(), ["Root", "Header"]);
    assert_eq!(cursor.pos(), 0);
    assert!(cursor.recorded_bits().is_empty());
    assert!(cursor.segments().is_empty());
}

#[hypothesis_test]
fn alpha_fixture_adapter_keeps_legacy_absolute_cursor_behavior() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/savegames/original/amazon_10_scrolls.d2s"),
    )
    .unwrap();
    let huffman = HuffmanTree::new();
    let items = d2r_core::item::Item::read_player_items(&bytes, &huffman, true).unwrap();
    for item in items.iter().take(2) {
        let start = item.range.start;
        let mut cursor = cursor(&bytes, start, bytes.len() as u64 * 8);
        let header = parse_header(
            &mut cursor,
            &HeaderAxiom::new(5, true),
            &huffman,
            Some((&bytes, start)),
        )
        .unwrap();
        assert!(header.save_is_alpha);
        // This public adapter's historical absolute-position behavior is preserved.
        assert_eq!(cursor.pos(), start);
        assert!(cursor.recorded_bits().is_empty());
        assert_eq!(cursor.context_stack(), ["Root"]);
        assert_eq!(cursor.segments().last().unwrap().start, start);
        assert_eq!(cursor.segments().last().unwrap().end, start);
    }
}
