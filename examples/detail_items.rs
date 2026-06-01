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

    for (idx, item) in items.iter().enumerate() {
        println!(
            "Item [{}]: {}, Qual: {:?}, Compact: {}, Socketed: {}, Quant: {:?}",
            idx, item.code, item.quality, item.is_compact, item.is_socketed, item.quantity
        );
        if !item.properties.is_empty() {
            println!("  Properties: {}", item.properties.len());
            for prop in &item.properties {
                println!("    Stat: {}, Val: {}", prop.stat_id, prop.raw_value);
            }
        }
    }
}
