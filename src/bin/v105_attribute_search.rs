use std::env;
use std::fs;
use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::io::Cursor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Usage: v105_attribute_search <file_path> <start_byte_offset> <target_value> [bit_width] [search_limit_bytes]");
        return Ok(());
    }

    let file_path = &args[1];
    let start_offset: usize = args[2].parse()?;
    let target_value: u64 = args[3].parse()?;
    let bit_width: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(9); // Default to 9 bits for stats
    let limit_bytes: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(200);

    let buffer = fs::read(file_path)?;
    println!("[Forensic] Searching for value {} (width {} bits) starting from byte offset {} (limit {} bytes)", target_value, bit_width, start_offset, limit_bytes);

    let max_bits = limit_bytes * 8;
    
    let mut found = false;
    for bit_idx in 0..max_bits {
        let slice = &buffer[start_offset..];
        let mut probe = BitReader::endian(Cursor::new(slice), LittleEndian);
        
        // Skip to current bit
        for _ in 0..bit_idx {
            if probe.read_bit().is_err() { break; }
        }

        // Manual read_bits implementation for compatibility
        let mut val: u64 = 0;
        let mut ok = true;
        for i in 0..bit_width {
            match probe.read_bit() {
                Ok(b) => {
                    if b {
                        val |= 1 << i;
                    }
                }
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }

        if ok && val == target_value {
            println!("MATCH FOUND: Bit Offset {} (Raw Bit Index {}, Byte-Relative {})", 
                (start_offset * 8) + bit_idx, 
                bit_idx,
                format!("{}.{}", bit_idx / 8, bit_idx % 8)
            );
            found = true;
        }
    }

    if !found {
        println!("No matches found for value {} in the search range.", target_value);
    }

    Ok(())
}
