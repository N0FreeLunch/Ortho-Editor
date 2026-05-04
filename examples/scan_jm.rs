use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::env;
use std::fs;
use std::io::Cursor;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: scan_jm <file_path>");
        return;
    }
    let fixture_path = &args[1];
    let bytes = fs::read(fixture_path).expect("Fixture not found");
    
    let limit = (bytes.len() * 8) as u64;
    
    println!("Scanning for JM marker (0x4D4A) 1-bit granular:");
    for bit_offset in 0..(limit - 16) {
        let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
        let _ = reader.skip(bit_offset as u32).unwrap();
        let val = reader.read::<16, u16>().unwrap();
        if val == 0x4D4A {
             println!("Found JM marker at bit {}", bit_offset);
        }
    }
}
