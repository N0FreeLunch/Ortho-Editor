use bitstream_io::{BitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::io::Cursor;

fn main() {
    let mut parser = ArgParser::new("d2item_bit_scan")
        .description("Brute force bit scan to find potential items in a save file.");
    parser.add_spec(ArgSpec::positional("file", "Path to save file"));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return;
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}\n\n{}", e, parser.usage());
            std::process::exit(1);
        }
    };

    let path = parsed.get("file").unwrap();
    let bytes = fs::read(path).expect("failed to read save file");
    let huffman = HuffmanTree::new();

    let mut starts = Vec::new();
    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];

    // Simple brute force bit scan
    for bit_pos in 0..(bytes.len() as u64 * 8 - 100) {
        let b_start = (bit_pos / 8) as usize;
        let b_off = (bit_pos % 8) as u32;

        let cursor = Cursor::new(&bytes[b_start..]);
        let reader = BitReader::endian(cursor, LittleEndian);
        let mut recorder = BitCursor::new(reader);
        if b_off > 0 {
            let _ = recorder.skip_and_record(b_off).ok();
        }

        if let Ok(item) = Item::from_reader_with_context(
            &mut recorder,
            &huffman,
            Some((&bytes, bit_pos)),
            is_alpha,
            false,
        ) {
            if recorder.pos() >= 32 {
                starts.push((bit_pos, item.code.clone()));
            }
        }
    }

    println!("Found {} items via scan:", starts.len());
    for (bit, code) in starts {
        println!("  Bit {}: code '{}'", bit, code);
    }
}
