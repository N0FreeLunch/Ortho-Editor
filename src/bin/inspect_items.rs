fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let file_path = "tests/fixtures/savegames/original/amazon_v105_slice2_equipment.d2s";
    let bytes = std::fs::read(file_path)?;
    let huffman = d2r_core::item::HuffmanTree::new();
    let items = d2r_core::item::Item::read_player_items(&bytes, &huffman, true)?;
    
    for (idx, item) in items.iter().enumerate() {
        println!("Item {}: code='{}', start={}, end={}, total_bits={}, bits_len={}, is_compact={}",
            idx, item.code, item.range.start, item.range.end, item.total_bits, item.bits.len(), item.header.is_compact);
        for (c_idx, child) in item.socketed_items.iter().enumerate() {
            println!("  Child {}: code='{}', start={}, end={}, total_bits={}, bits_len={}",
                c_idx, child.code, child.range.start, child.range.end, child.total_bits, child.bits.len());
        }
    }
    Ok(())
}
