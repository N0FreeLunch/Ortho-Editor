use d2r_core::item::{HuffmanTree, Item};
use std::fs;

fn main() {
    let bytes =
        fs::read("tests/fixtures/savegames/original/amazon_authority_runeword.d2s").unwrap();
    let huffman = HuffmanTree::new();

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    match Item::read_player_items(&bytes, &huffman, version == 105) {
        Ok(items) => {
            println!("Parsed {} items.", items.len());
            for (i, item) in items.iter().enumerate() {
                println!(
                    "Item {}: code={}, start={}, bin_len={} bits",
                    i,
                    item.code,
                    item.range.start,
                    item.bits.len()
                );
                println!(
                    "  Header: flags=0x{:08X}, v={}, m={}, l={}, x={}, has_checksum={}",
                    item.header.flags,
                    item.header.version,
                    item.header.mode,
                    item.header.location,
                    item.header.x,
                    item.header.has_checksum
                );
            }
        }
        Err(e) => {
            println!("Error parsing items: {}", e);
        }
    }
}
