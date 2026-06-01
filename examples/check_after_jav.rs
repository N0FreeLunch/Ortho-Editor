use d2r_core::item::{HuffmanTree, Item};
use std::fs;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");

    let huffman = HuffmanTree::new();
    let items = Item::read_player_items(&bytes[jm_pos..], &huffman, true).expect("Parsing failed");

    if let Some(last) = items.last() {
        println!("Last item: {}, range: {:?}", last.code, last.range);
        let end_byte = (last.range.end as usize + 7) / 8;
        let jm_data_start = jm_pos + 4;
        let absolute_end = jm_data_start + end_byte;

        println!("Absolute end byte in file: 0x{:X}", absolute_end);

        let remaining = &bytes[absolute_end..];
        println!(
            "Remaining bytes (up to 64): {:02X?}",
            &remaining[..remaining.len().min(64)]
        );

        // Search for next "JM"
        for i in 0..remaining.len().saturating_sub(1) {
            if remaining[i] == b'J' && remaining[i + 1] == b'M' {
                println!("Found next JM at offset 0x{:X} relative to end", i);
                break;
            }
        }
    }
}
