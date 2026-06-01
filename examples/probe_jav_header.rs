use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::fs;
use std::io::Cursor;

fn main() {
    let fixture_path = "tests/fixtures/savegames/original/amazon_10_scrolls.d2s";
    let bytes = fs::read(fixture_path).expect("Fixture not found");

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("JM header not found");

    let start_byte = jm_pos + 4 + 130; // 1040 bits / 8 = 130 bytes
    let data = &bytes[start_byte..];

    let mut reader = BitReader::endian(Cursor::new(data), LittleEndian);

    println!("Bits from 1040 (jav start):");
    for i in 0..80 {
        let b = reader.read_bit().unwrap();
        print!("{}", if b { "1" } else { "0" });
        if (i + 1) % 8 == 0 {
            print!(" ");
        }
        if (i + 1) % 32 == 0 {
            println!();
        }
    }
    println!();
}
