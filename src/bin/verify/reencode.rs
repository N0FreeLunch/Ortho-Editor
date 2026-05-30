use std::fs;
use d2r_core::item::{Item, HuffmanTree};

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_authority_runeword.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");
    
    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");
    
    let huffman = HuffmanTree::new();
    let items = Item::read_player_items(&bytes, &huffman, true).expect("Parsing failed");
    let reserialized_items = Item::serialize_section(&items, &huffman, true).expect("Serialization failed");
    
    // Write original payload and reserialized payload to files
    let original_payload = &bytes[jm_pos + 4..];
    fs::write("tmp/original_items.bin", original_payload).unwrap();
    fs::write("tmp/reserialized_items.bin", reserialized_items).unwrap();
    
    println!("Successfully dumped original and reserialized item chunks to tmp/ !");
}
