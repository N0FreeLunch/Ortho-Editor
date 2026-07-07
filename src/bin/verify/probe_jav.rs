use d2r_core::domain::forensic::v105::axioms::get_v105_target_width;
use d2r_core::domain::item::Item;
use d2r_core::domain::item::serialization::HuffmanTree;
use std::fs;

fn main() {
    let bytes = fs::read("tests/fixtures/savegames/original/amazon_10_scrolls.d2s").unwrap();
    let huffman = HuffmanTree::new();
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let items = Item::read_player_items(&bytes, &huffman, version == 105).unwrap();
    let buc = items.iter().find(|i| i.code.trim() == "buc").unwrap();

    let target_width =
        get_v105_target_width(buc.header.version, &buc.code, buc.header.flags, Some(0));
    println!("get_v105_target_width for buc: {}", target_width);
}
