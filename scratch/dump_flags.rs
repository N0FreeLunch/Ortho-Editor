use d2r_core::domain::item::serialization::{peek_item_header_at, HuffmanTree};
use std::fs;

fn main() {
    let path = "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest1_LarzukSocketAdded.d2s";
    let bytes = fs::read(path).unwrap();
    let huffman = HuffmanTree::new();
    
    // Find JM section
    let mut pos = 0;
    for i in 0..bytes.len()-2 {
        if bytes[i] == b'J' && bytes[i+1] == b'M' {
            pos = (i * 8) as u64;
            // The JM header itself (16 bits)
            pos += 16;
            // Number of items (16 bits)
            pos += 16;
            break;
        }
    }
    
    println!("Dumping items from bit {}", pos);
    let mut bit = pos;
    for i in 0..5 {
        if let Some((mode, loc, x, code, flags, version, is_compact, header_len, _)) = peek_item_header_at(&bytes, bit, &huffman, true) {
            println!("Item {}: code='{}', flags=0x{:08X}, version={}, is_compact={}", i, code, flags, version, is_compact);
            // Rough length estimate to jump to next item
            bit += 128; // This is wrong but we'll try to find next JM
            while bit < (bytes.len() * 8) as u64 {
                if bit % 8 == 0 {
                    let b_idx = (bit / 8) as usize;
                    if b_idx + 1 < bytes.len() && bytes[b_idx] == b'J' && bytes[b_idx+1] == b'M' {
                        // Found next item header
                        break;
                    }
                }
                bit += 1;
            }
        } else {
            println!("Item {}: Failed to peek", i);
            break;
        }
    }
}
