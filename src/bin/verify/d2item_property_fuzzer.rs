use std::env;
use std::fs;
use std::path::Path;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use d2r_core::domain::header::entity::calculate_alpha_v105_checksum;

fn main() {
    let mut parser = ArgParser::new("d2item_property_fuzzer");
    parser.add_spec(ArgSpec::positional("fixture", "Path to save file"));
    parser.add_spec(ArgSpec::option("target", Some('t'), Some("target"), "Index of target item (0-based)"));
    parser.add_spec(ArgSpec::option("bit-range", Some('r'), Some("bit-range"), "Range of bits to flip (start..end)"));
    parser.add_spec(ArgSpec::flag("force-save-failed", None, Some("force-save-failed"), "Save even if parsing fails"));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let file_path = parsed.get("fixture").unwrap();
    let target_idx: usize = parsed.get("target").map(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0);
    let bit_range_str = parsed.get("bit-range").map(|v| v.as_str()).unwrap_or("0..1");
    
    let (bit_start, bit_end) = parse_range(bit_range_str).unwrap_or((0, 1));

    let original_bytes = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[ERROR] Failed to read file {}: {}", file_path, e);
            std::process::exit(1);
        }
    };

    let markers = find_jm_markers(&original_bytes);
    if target_idx >= markers.len() {
        eprintln!("[ERROR] Target item index {} out of range (found {} items).", target_idx, markers.len());
        std::process::exit(1);
    }

    let item_byte_offset = markers[target_idx];
    let item_bit_offset = item_byte_offset * 8;

    println!("--- Alpha v105 Item Property Poke-Test Fuzzer ---");
    println!("Fixture: {}", file_path);
    println!("Target Item: #{} (at byte 0x{:X})", target_idx, item_byte_offset);
    println!("Fuzzing Bit Range: {}..{}", bit_start, bit_end);

    let out_dir = Path::new("tmp/fuzz_outputs");
    if !out_dir.exists() {
        fs::create_dir_all(out_dir).unwrap();
    }

    for bit_offset in bit_start..bit_end {
        let mut mutated = original_bytes.clone();
        
        // Flip the bit
        let absolute_bit = item_bit_offset + bit_offset;
        let byte_idx = absolute_bit / 8;
        let bit_in_byte = absolute_bit % 8;
        
        if byte_idx >= mutated.len() {
            eprintln!("[WARN] Bit offset {} exceeds file size. Skipping.", bit_offset);
            continue;
        }
        
        mutated[byte_idx] ^= 1 << bit_in_byte;

        // Re-calculate checksum if header mutated
        // Alpha v105 Header: Flags(32), Checksum(8), Version(3)
        // Checksum depends on Flags and Version.
        
        let flags = read_u32_at_bit(&mutated, item_bit_offset);
        let version = read_u8_at_bit(&mutated, item_bit_offset + 40, 3);
        
        let new_checksum = calculate_alpha_v105_checksum(flags, version);
        write_u8_at_bit(&mut mutated, item_bit_offset + 32, 8, new_checksum);

        let out_name = format!("fuzz_item{}_bit{}.d2s", target_idx, bit_offset);
        let out_path = out_dir.join(out_name);
        fs::write(&out_path, &mutated).unwrap();
        println!("  [SAVED] {}", out_path.display());
    }

    println!("Fuzzing complete. Results in tmp/fuzz_outputs/");
}

fn find_jm_markers(bytes: &[u8]) -> Vec<usize> {
    let mut markers = Vec::new();
    if bytes.len() < 2 { return markers; }
    for i in 0..bytes.len() - 1 {
        if bytes[i] == 0x4A && bytes[i + 1] == 0x4D {
            markers.push(i);
        }
    }
    markers
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = s.split("..").collect();
    if parts.len() == 2 {
        let start = parts[0].parse().ok()?;
        let end = parts[1].parse().ok()?;
        Some((start, end))
    } else {
        None
    }
}

fn read_u32_at_bit(bytes: &[u8], bit_offset: usize) -> u32 {
    let mut val: u32 = 0;
    for i in 0..32 {
        let abs_bit = bit_offset + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = abs_bit % 8;
        if byte_idx < bytes.len() {
            if (bytes[byte_idx] & (1 << bit_in_byte)) != 0 {
                val |= 1 << i;
            }
        }
    }
    val
}

fn read_u8_at_bit(bytes: &[u8], bit_offset: usize, count: usize) -> u8 {
    let mut val: u8 = 0;
    for i in 0..count {
        let abs_bit = bit_offset + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = abs_bit % 8;
        if byte_idx < bytes.len() {
            if (bytes[byte_idx] & (1 << bit_in_byte)) != 0 {
                val |= 1 << i;
            }
        }
    }
    val
}

fn write_u8_at_bit(bytes: &mut [u8], bit_offset: usize, count: usize, val: u8) {
    for i in 0..count {
        let abs_bit = bit_offset + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = abs_bit % 8;
        if byte_idx < bytes.len() {
            if (val & (1 << i)) != 0 {
                bytes[byte_idx] |= 1 << bit_in_byte;
            } else {
                bytes[byte_idx] &= !(1 << bit_in_byte);
            }
        }
    }
}
