use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::fs;
use std::io::Cursor;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");

    let start_byte = jm_pos + 4 + 130; // 1040 bits
    let data = &bytes[start_byte..];

    let mut reader = BitReader::endian(Cursor::new(data), LittleEndian);
    let _ = reader.skip(53 + 23).unwrap(); // Header + jav code

    println!("Probing jav properties from bit 76:");
    for i in 0..100 {
        let id = reader.read::<9, u16>().unwrap();
        if id == 0x1FF {
            println!("FOUND TERMINATOR AT Prop [{}]", i);
            let bit = reader.read_bit().unwrap();
            println!("  Terminator Bit: {}", bit);
            break;
        }
        let val = reader.read::<1, u8>().unwrap();
        println!("Prop [{}]: ID={}, Val={}", i, id, val);
    }
}
