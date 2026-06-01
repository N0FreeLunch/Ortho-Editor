use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::HuffmanTree;
use std::fs;
use std::io::Cursor;

fn main() {
    let bytes =
        fs::read("tests/fixtures/savegames/original/amazon_authority_runeword.d2s").unwrap();
    let huffman = HuffmanTree::new();

    let start_0 = 7256;
    let target_codes = ["hp1 ", "mp1 ", "spr ", "bsh "];

    println!("--- Scanning for Item 1 Start ---");
    for stride in 80..100 {
        let start_1 = start_0 + stride;
        for h_len in &[45, 53, 61] {
            let mut h_reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);
            if h_reader.skip((start_1 + h_len) as u32).is_err() {
                continue;
            }
            let mut cursor = BitCursor::new(h_reader);

            let mut code = String::new();
            for _ in 0..4 {
                if let Ok(ch) = huffman.decode_recorded(&mut cursor) {
                    code.push(ch);
                }
            }

            for &target in &target_codes {
                if code == target {
                    println!(
                        "  MATCH at Stride={}: Header={}, Code='{}'",
                        stride, h_len, code
                    );
                }
            }
        }
    }
}
