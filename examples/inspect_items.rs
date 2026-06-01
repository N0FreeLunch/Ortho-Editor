use d2r_core::item::{HuffmanTree, Item};
use std::fs;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");

    let count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
    println!("Expected items (JM count): {}", count);

    let huffman = HuffmanTree::new();
    let items = Item::read_player_items(&bytes[jm_pos..], &huffman, true).expect("Parsing failed");

    println!("Top-level items recovered: {}", items.len());

    let mut total_count = 0;
    fn count_items(items: &[Item], total: &mut usize) {
        for item in items {
            *total += 1;
            println!(
                "Item: {} (nested: {})",
                item.code,
                item.socketed_items.len()
            );
            count_items(&item.socketed_items, total);
        }
    }

    count_items(&items, &mut total_count);
    println!("Total items recovered (including nested): {}", total_count);
}
