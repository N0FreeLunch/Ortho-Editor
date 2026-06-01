use bitstream_io::{BitRead, BitReader as IoBitReader, LittleEndian};
use std::fs;
use std::io::Cursor;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        println!("Usage: d2item_bit_search <save_file> <start_bit> <bit_count>");
        return;
    }
    let save_file = &args[1];
    let start_bit: u64 = args[2].parse().expect("Invalid start_bit");
    let bit_count: u64 = args[3].parse().expect("Invalid bit_count");

    let bytes = fs::read(save_file).expect("Failed to read save file");

    println!("Dumping {} bits from bit {}...", bit_count, start_bit);

    let start_byte = (start_bit / 8) as usize;
    let bit_offset = (start_bit % 8) as u32;

    if start_byte >= bytes.len() {
        println!("Error: start_bit is out of range.");
        return;
    }

    let mut reader = IoBitReader::endian(Cursor::new(&bytes[start_byte..]), LittleEndian);
    for _ in 0..bit_offset {
        let _ = reader.read_bit().ok();
    }

    let mut bits = Vec::new();
    for _ in 0..bit_count {
        if let Ok(b) = reader.read_bit() {
            bits.push(if b { '1' } else { '0' });
        } else {
            break;
        }
    }

    let bit_string: String = bits.iter().collect();

    // Look for various terminators (9 bits)
    for terminator in ["111111111", "1111111110"] {
        if let Some(pos) = bit_string.find(terminator) {
            println!(
                "Found pattern {} at relative bit offset {}",
                terminator, pos
            );
        }
    }

    // Print in groups of 8 for easy byte visualization
    for (i, chunk) in bits.chunks(8).enumerate() {
        let s: String = chunk.iter().collect();
        print!("{} ", s);
        if (i + 1) % 4 == 0 {
            println!();
        }
    }
    println!();
}
