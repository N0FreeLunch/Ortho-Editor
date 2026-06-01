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
    let data = &bytes[start_byte..];

    let mut reader = BitReader::endian(Cursor::new(data), LittleEndian);
    let _ = reader.skip(53 + 23); // Skip header and jav code
    let val = reader.read::<32, u32>().unwrap();
    println!("32 bits after Huffman: 0x{:08X}", val);

    // Also check 17 bits
    let mut reader2 = BitReader::endian(Cursor::new(data), LittleEndian);
    let _ = reader2.skip(53 + 23);
    let val2 = reader2.read::<17, u32>().unwrap();
    println!("17 bits after Huffman: 0x{:05X}", val2);
}
