use d2r_core::item::HuffmanTree;
use std::fs;
use std::env;

fn main() {
    let path = "../d2r-core/tests/fixtures/savegames/original/amazon_initial.d2s";
    let bytes = fs::read(path).expect("Failed to read save file");
    let huffman = HuffmanTree::new();
    let is_alpha = true;
    
    let jm_positions = d2r_core::save::find_jm_markers(&bytes);
    if jm_positions.is_empty() {
        println!("No JM markers found");
        return;
    }
    
    let pos = jm_positions[0];
    let section_bytes = &bytes[pos..];
    let section_bit_offset = (pos as u64) * 8;
    
    let markers = d2r_core::domain::item::scanner::scan_item_markers(section_bytes, &huffman, is_alpha, section_bit_offset, None);
    
    println!("Markers found in amazon_initial.d2s (starting at bit {}):", section_bit_offset);
    for (i, m) in markers.iter().enumerate() {
        println!("  Item {:>2}: bit {:>12}", i, m);
    }
}
