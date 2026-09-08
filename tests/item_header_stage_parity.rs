mod common;

use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::domain::inventory::InventoryGrid;
use d2r_core::item::{HuffmanTree, Item};
use std::io::Cursor;

fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

#[d2r_macros::hypothesis_test]
fn full_parser_frozen_samples() {
    let huffman = HuffmanTree::new();
    for (path, expected) in [
        (
            "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
            3851352285737821979u64,
        ),
        (
            "tests/fixtures/savegames/gameplay/normal/act2/TESTDRUID_Q2_CubeFoundNoCainTalk.d2s",
            13385155582798093372u64,
        ),
    ] {
        let bytes = std::fs::read(common::repo_path(path)).unwrap();
        let items = Item::read_player_items(&bytes, &huffman, true).unwrap();
        let mut signature = Vec::new();
        for item in &items {
            signature.extend_from_slice(
                format!("{:?}|{:?}|{}\n", item.header, item.range, item.code).as_bytes(),
            );
        }
        signature.extend_from_slice(
            format!(
                "{:?}",
                InventoryGrid::validate_all_containers_integrity(&items)
            )
            .as_bytes(),
        );
        signature.extend_from_slice(&Item::serialize_section(&items, &huffman, true).unwrap());
        let actual = digest(&signature);
        println!("{path}: count={} signature={actual}", items.len());
        assert_eq!(actual, expected, "{path}");
    }
}

#[d2r_macros::hypothesis_test]
fn shared_header_stage_matches_same_context_full_entry() {
    let bytes = std::fs::read(common::repo_path(
        "tests/fixtures/savegames/original/amazon_10_scrolls.d2s",
    ))
    .unwrap();
    let huffman = HuffmanTree::new();
    let items = Item::read_player_items(&bytes, &huffman, true).unwrap();
    let start = items[0].range.start;
    // Baseline offsets are test anchors, not independent scanner evidence.
    for use_context in [false, true] {
        for hint in [None, Some(""), Some(items[0].code.as_str())] {
            let make_cursor = || {
                let mut reader = BitReader::endian(Cursor::new(bytes.as_slice()), LittleEndian);
                reader.skip(u32::try_from(start).unwrap()).unwrap();
                let mut cursor = BitCursor::new(reader);
                cursor.set_pos(start);
                cursor.set_limit(bytes.len() as u64 * 8);
                cursor.set_trace(true);
                cursor
            };
            let ctx = use_context.then_some((bytes.as_slice(), start));
            let mut header_cursor = make_cursor();
            let stage =
                Item::read_header_stage(&mut header_cursor, &huffman, ctx, true, 0, None, hint)
                    .unwrap();
            let mut full_cursor = make_cursor();
            let full = Item::from_reader_with_context(
                &mut full_cursor,
                &huffman,
                ctx,
                true,
                0,
                None,
                hint,
            )
            .unwrap();
            // Body decoding can refine id and compact status. Only compare
            // header-stage placement fields, not the finalized header object.
            let placement = |h: &d2r_core::domain::header::entity::ItemHeader| {
                (h.mode, h.location, h.x, h.y, h.page, h.flags, h.version)
            };
            assert_eq!(placement(&stage.header), placement(&full.header));
            assert!(
                header_cursor
                    .context_stack()
                    .iter()
                    .any(|label| label == "Root")
            );
            assert!(
                header_cursor.segments().iter().all(|segment| {
                    segment.label != "Stats" && segment.label != "ExtendedStats"
                })
            );
            let header_end = header_cursor
                .segments()
                .iter()
                .rev()
                .find(|segment| segment.label == "Header")
                .unwrap()
                .end;
            assert_eq!(header_cursor.pos(), header_end);
            let count = header_cursor.segments().len();
            assert!(full_cursor.segments().len() >= count);
            for (stage_segment, full_segment) in header_cursor
                .segments()
                .iter()
                .zip(&full_cursor.segments()[..count])
            {
                assert_eq!(stage_segment.start, full_segment.start);
                assert_eq!(stage_segment.end, full_segment.end);
                assert_eq!(stage_segment.label, full_segment.label);
                assert_eq!(stage_segment.depth, full_segment.depth);
            }
        }
    }
}
