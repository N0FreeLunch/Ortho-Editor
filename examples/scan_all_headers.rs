use d2r_core::item::{Item, HuffmanTree, is_plausible_item_header};
use std::fs;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");
    
    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");
    
    let huffman = HuffmanTree::new();
    let section_bytes = &bytes[jm_pos + 4..];
    let limit = (section_bytes.len() * 8) as u64;
    
    println!("Scanning for item headers in section 0 (1-bit granular)...");
    for bit_offset in 0..limit {
        if let Some((mode, location, x, code, flags, version, is_compact, header_bits, nudge)) = 
            d2r_core::item::peek_item_header_at(section_bytes, bit_offset, &huffman, true) {
            if d2r_core::item::is_plausible_item_header(mode, location, &code, flags, version, true) {
                if version == 0 || version == 1 || version == 4 || version == 5 || version == 6 || version == 7 {
                    println!("Found plausible header at bit {}: code='{}', compact={}, mode={}, loc={}, version={}", 
                        bit_offset, code, is_compact, mode, location, version);
                }
            }
        }
    }
}
