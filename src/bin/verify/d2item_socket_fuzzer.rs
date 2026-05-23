use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::item::{HuffmanTree, Item};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // 1. Setup paths and constants
    // Use the absolute path to ensure fixture is found regardless of CWD
    let mut fixture_path = PathBuf::from("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
    if !fixture_path.exists() {
        // Fallback for running from project root
        fixture_path = PathBuf::from("d2r-core/tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
    }
    
    let bytes = fs::read(&fixture_path).map_err(|e| anyhow::anyhow!("Failed to read {}: {}", fixture_path.display(), e))?;
    
    // 2. Find first JM marker (anchor)
    let jm_byte_pos = (0..bytes.len() - 2)
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("No JM marker found");
    let anchor = (jm_byte_pos as u64) * 8;
    
    // 3. Define target seam (relative 805 from anchor as verified in audit)
    let seam_offset = 805;
    let seam = anchor + seam_offset;
    
    println!("Fixture: {}", fixture_path.display());
    println!("Anchor (JM): {} (byte {}), Seam: {} (relative {})", anchor, jm_byte_pos, seam, seam_offset);
    println!("{:>5} | {:>15} | {:>5} | {:>8} | {:>10}", "Shift", "Children", "Mode", "Symmetry", "Status");
    println!("{:-<65}", "");

    let huffman = HuffmanTree::new();
    let is_alpha = true;

    for shift in 0..=48 {
        // Construct modified bitstream
        let fuzzed_bytes = if shift == 0 {
            bytes.clone()
        } else {
            inject_bits(&bytes, seam, shift)
        };
        
        // Slice to item section
        let section_bytes = &fuzzed_bytes[jm_byte_pos..];
        
        // Parse from anchor
        let res = Item::read_section_ext(
            section_bytes,
            anchor,
            15, // Plenty of items
            &huffman,
            is_alpha,
            false,
        );

        match res {
            Ok(items) => {
                if shift == 0 {
                    for (i, it) in items.iter().enumerate() {
                        println!("DEBUG: Item {} code='{}' socketed={}", i, it.code(), it.socketed_items.len());
                        for (j, child) in it.socketed_items.iter().enumerate() {
                            println!("  DEBUG: Child {} code='{}'", j, child.code());
                        }
                    }
                }
                // Find the xrs item that is supposed to have socketed items
                let parent = items.iter().find(|it| it.code().trim().to_lowercase() == "xrs" && it.socketed_items.len() > 0);
                
                if let Some(p) = parent {
                    let child_count = p.socketed_items.len();
                    let mode = p.header.mode;
                    let symmetry = "N/A"; // Placeholder
                    println!("{:5} | {:>12}/3 | {:5} | {:>8} | OK", shift, child_count, mode, symmetry);
                } else {
                    // Try to find ANY xrs if no child-bearing one is found
                    let any_xrs = items.iter().find(|it| it.code().trim().to_lowercase() == "xrs");
                    if let Some(p) = any_xrs {
                         println!("{:5} | {:>12}/3 | {:5} | {:>8} | NO_CHILDREN", shift, p.socketed_items.len(), p.header.mode, "-");
                    } else {
                         println!("{:5} | {:>15} | {:5} | {:>8} | NO_XRS", shift, "0/3", "-", "-");
                    }
                }
            }
            Err(e) => {
                println!("{:5} | {:>15} | {:5} | {:>8} | ERROR: {}", shift, "0/3", "-", "-", e);
            }
        }
    }

    Ok(())
}

fn inject_bits(original: &[u8], at_bit: u64, count: u32) -> Vec<u8> {
    let mut bits = Vec::new();
    for &b in original {
        for i in 0..8 {
            bits.push((b >> i) & 1 == 1);
        }
    }
    
    let mut new_bits = Vec::new();
    if at_bit as usize > bits.len() {
        return original.to_vec();
    }
    
    new_bits.extend_from_slice(&bits[..at_bit as usize]);
    for _ in 0..count {
        new_bits.push(false); // Inject zeros
    }
    new_bits.extend_from_slice(&bits[at_bit as usize..]);
    
    // Convert back to bytes
    let mut out_bytes = Vec::new();
    for chunk in new_bits.chunks(8) {
        let mut b = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                b |= 1 << i;
            }
        }
        out_bytes.push(b);
    }
    out_bytes
}
