use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::env;
use std::fs;
use std::io::Cursor;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Usage: gf_bit_print <file_path> <bit_offset> <bit_count>");
        return;
    }

    let file_path = &args[1];
    let bit_offset: usize = args[2].parse().unwrap();
    let bit_count: usize = args[3].parse().unwrap();

    let bytes = fs::read(file_path).unwrap();
    
    let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
    
    // Skip to bit_offset
    for _ in 0..bit_offset {
        let _ = reader.read_bit().unwrap();
    }

    println!("=== Bits from {} (count {}) ===", bit_offset, bit_count);
    for i in 0..bit_count {
        if i > 0 && i % 8 == 0 {
            print!(" ");
        }
        if i > 0 && i % 64 == 0 {
            println!();
        }
        let b = reader.read_bit().unwrap();
        print!("{}", if b { "1" } else { "0" });
    }
    println!("\n");
}
