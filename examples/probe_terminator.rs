use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::fs;
use std::io::Cursor;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");

    let start_byte = jm_pos + 4 + 130; // jav start
    if start_byte >= bytes.len() {
        println!("jav start byte 0x{:X} is out of bounds", start_byte);
        return;
    }
    let data = &bytes[start_byte..];
    println!("Data len: {} bytes", data.len());

    for bit_offset in 0..(data.len() * 8 - 9) {
        let mut reader = BitReader::endian(Cursor::new(data), LittleEndian);
        let _ = reader.skip(bit_offset as u32);
        let val = reader.read::<9, u32>().unwrap();
        if val == 0x1FF {
            println!("Found 0x1FF at bit offset {}", bit_offset);
            // Check next bit
            let next_bit = reader.read_bit().unwrap();
            println!("  Next bit: {}", next_bit);
        }
    }
}
