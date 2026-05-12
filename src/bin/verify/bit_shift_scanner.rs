use bitstream_io::{BitRead, BitReader as IoBitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::{HuffmanTree, Item};
use std::env;
use std::fs;
use std::io::Cursor;
use std::process;
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};

fn main() {
    let mut parser = ArgParser::new("BitShiftScanner")
        .description("Scans for valid item headers by shifting the starting bit offset around the JM marker");

    parser.add_spec(ArgSpec::positional("save_file", "path to the save file (.d2s)"));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let path = parsed.get("save_file").unwrap();
    let bytes = fs::read(path).expect("failed to read save file");

    let jm_pos = (0..bytes.len() - 2)
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("No JM marker found");
    
    let huffman = HuffmanTree::new();
    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];

    for shift in -16i64..=16i64 {
        let bit_start = ((jm_pos + 4) * 8) as i64 + shift;
        if bit_start < 0 || bit_start >= (bytes.len() * 8) as i64 { continue; }

        let mut reader = IoBitReader::endian(Cursor::new(&bytes), LittleEndian);
        let _ = reader.skip(bit_start as u32);
        let mut cursor = BitCursor::new(reader);

        match Item::from_reader_with_context(&mut cursor, &huffman, Some((&bytes, bit_start as u64)), is_alpha, false, None) {
            Ok(item) => {
                println!("  [Shift {:+3}] SUCCESS: '{}' (len={} bits, flags=0x{:08X}, compact={})",
                    shift, item.code, cursor.pos(), item.flags, item.is_compact);
            }
            Err(_) => {}
        }
    }
}
