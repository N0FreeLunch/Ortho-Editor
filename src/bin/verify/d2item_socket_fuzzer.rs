use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::item::{HuffmanTree, Item};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // 1. Setup paths and constants
    let args: Vec<String> = std::env::args().collect();
    let mut fixture_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        // Default paths if no arg provided
        let mut p = PathBuf::from("tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        if !p.exists() {
            p = PathBuf::from("d2r-core/tests/fixtures/savegames/original/amazon_authority_runeword.d2s");
        }
        p
    };
    
    if !fixture_path.exists() {
        anyhow::bail!("Fixture path not found: {}", fixture_path.display());
    }
    
    let bytes = fs::read(&fixture_path).map_err(|e| anyhow::anyhow!("Failed to read {}: {}", fixture_path.display(), e))?;
    
    // 2. Find first JM marker (anchor)
    let jm_byte_pos = (0..bytes.len() - 2)
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("No JM marker found");
    let anchor = (jm_byte_pos as u64) * 8;
    
    // 3. Discover target seam dynamically
    let huffman = HuffmanTree::new();
    let is_alpha = true;
    
    let initial_items = Item::read_section_ext(
        &bytes[jm_byte_pos..],
        anchor,
        15, // Plenty of items
        &huffman,
        is_alpha,
        false,
    ).expect("Initial parse failed to find items for seam discovery");

    // We look for the "parent" item - usually the first one that is a candidate for sockets or a runeword
    let parent = initial_items.iter().find(|it| {
        let code = it.code().trim().to_lowercase();
        code == "xrs" || (it.header.is_runeword && it.socketed_items.len() > 0)
    }).or_else(|| initial_items.first()) // Fallback to first item if no specific xrs found
    .expect("No items found to determine seam");

    // Dynamic Seam Discovery (Marker + Terminator Heuristic)
    let section_bits = to_bits(&bytes[jm_byte_pos..]);
    let rel_parent_start = parent.range.start - anchor;
    
    // Search after header + gap + code (roughly 127 bits for xrs)
    let search_start = (rel_parent_start + 100) as usize;
    
    let marker_seam = find_marker(&section_bits, search_start);
    let term_seam = find_terminator_at(&section_bits, search_start);
    
    let (seam_offset, seam_source) = match (marker_seam, term_seam) {
        (Some(m), Some(t)) => {
            if m < t { (m as u64, "marker_jm") } else { (t as u64, "terminator_0x1FF") }
        },
        (Some(m), None) => (m as u64, "marker_jm"),
        (None, Some(t)) => (t as u64, "terminator_0x1FF"),
        (None, None) => (parent.range.end - anchor, "item_range_end_fallback"),
    };

    let seam = anchor + seam_offset;
    
    println!("Fixture: {}", fixture_path.display());
    println!("Anchor (JM): {} (byte {}), Seam: {} (relative {}), Source: {}", anchor, jm_byte_pos, seam, seam_offset, seam_source);
    println!("{:>5} | {:>15} | {:>5} | {:>8} | {:>10}", "Shift", "Children", "Mode", "Symmetry", "Status");
    println!("{:-<65}", "");

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
                    println!("seam={} source={} shift={} parsed_children={}/3 mode={} status=ok", seam, seam_source, shift, child_count, mode);
                } else {
                    // Try to find ANY xrs if no child-bearing one is found
                    let any_xrs = items.iter().find(|it| it.code().trim().to_lowercase() == "xrs");
                    if let Some(p) = any_xrs {
                         println!("seam={} source={} shift={} parsed_children={}/3 mode={} status=no_children", seam, seam_source, shift, p.socketed_items.len(), p.header.mode);
                    } else {
                         println!("seam={} source={} shift={} parsed_children=0/3 mode=- status=no_xrs", seam, seam_source, shift);
                    }
                }
            }
            Err(e) => {
                println!("seam={} source={} shift={} parsed_children=0/3 mode=- status=error:{}", seam, seam_source, shift, e);
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

fn to_bits(bytes: &[u8]) -> Vec<bool> {
    let mut bits = Vec::new();
    for &b in bytes {
        for i in 0..8 {
            bits.push((b >> i) & 1 == 1);
        }
    }
    bits
}

fn find_marker(bits: &[bool], start_at: usize) -> Option<usize> {
    if bits.len() < 16 || start_at + 16 > bits.len() { return None; }
    // JM marker is 0x4A, 0x4D
    // In bits (LSB first):
    // 0x4A = 01001010 => 0, 1, 0, 1, 0, 0, 1, 0
    // 0x4D = 01001101 => 1, 0, 1, 1, 0, 0, 1, 0
    let marker_bits = [
        false, true, false, true, false, false, true, false,
        true, false, true, true, false, false, true, false,
    ];
    
    for i in (start_at..=(bits.len() - 16)).step_by(1) {
        if bits[i..i+16] == marker_bits {
            return Some(i);
        }
    }
    None
}

fn find_terminator_at(bits: &[bool], start_at: usize) -> Option<usize> {
    if bits.len() < 9 || start_at + 9 > bits.len() { return None; }
    for i in start_at..=(bits.len() - 9) {
        let mut all_ones = true;
        for j in 0..9 {
            if !bits[i + j] {
                all_ones = false;
                break;
            }
        }
        if all_ones {
            return Some(i);
        }
    }
    None
}

fn find_terminator(bits: &[bool]) -> Option<usize> {
    find_terminator_at(bits, 0)
}
