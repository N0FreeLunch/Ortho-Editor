use bitstream_io::{BitRead, BitReader as IoBitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::{HuffmanTree, Item};
use std::env;
use std::fs;
use std::io::Cursor;
use std::process;
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};

fn main() {
    let mut parser = ArgParser::new("BitNudgeExplorer")
        .description("Explores bit-level nudges for items in a D2R save file to find valid alignment");

    parser.add_spec(ArgSpec::positional("save_file", "path to the save file (.d2s)"));
    parser.add_spec(ArgSpec::positional("item_index", "optional target item index to inspect").optional());

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
    let target_idx = parsed.get("item_index").and_then(|s| s.parse::<usize>().ok());
    let bytes = fs::read(path).expect("failed to read save file");

    let jm_pos = (0..bytes.len() - 2)
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("No JM marker found");
    let count = u16::from_le_bytes([bytes[jm_pos + 2], bytes[jm_pos + 3]]);
    println!("JM at byte {}, item count {}", jm_pos, count);

    let huffman = HuffmanTree::new();
    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];

    let start_bit = (jm_pos + 4) as u64 * 8;
    let mut bit_pos = 0u64;

    for i in 0..count {
        let current_idx = i as usize;
        if let Some(target) = target_idx {
            if current_idx != target {
                // Skip items until target
                let mut reader = IoBitReader::endian(Cursor::new(&bytes[jm_pos + 4..]), LittleEndian);
                let _ = reader.skip(bit_pos as u32);
                let mut cursor = BitCursor::new(reader);
                if let Ok(_) = Item::from_reader_with_context(&mut cursor, &huffman, Some((&bytes, start_bit + bit_pos)), is_alpha, current_idx == 0, None) {
                    bit_pos += cursor.pos();
                    continue;
                } else {
                    break;
                }
            }
        }

        println!("\nExploring Item {} at bit offset {}:", current_idx, start_bit + bit_pos);

        // Try nudges
        for nudge in -4i64..=4i64 {
            let nudged_start = (bit_pos as i64 + nudge) as u64;
            if nudged_start >= (bytes.len() * 8) as u64 { continue; }

            let mut reader = IoBitReader::endian(Cursor::new(&bytes[jm_pos + 4..]), LittleEndian);
            let _ = reader.skip(nudged_start as u32);
            let mut cursor = BitCursor::new(reader);

            match Item::from_reader_with_context(&mut cursor, &huffman, Some((&bytes, start_bit + nudged_start)), is_alpha, current_idx == 0, None) {
                Ok(item) => {
                    println!("  [Nudge {:+2}] SUCCESS: '{}' (len={} bits)", nudge, item.code, cursor.pos());    
                    if nudge == 0 || target_idx.is_some() {
                        print_item_segments(&cursor);
                    }
                }
                Err(e) => {
                    println!("  [Nudge {:+2}] FAILED: {}", nudge, e);
                }
            }
        }

        // Only explore one item if target_idx is set
        if target_idx.is_some() {
            break;
        }

        // Move to next item normally
        let mut reader = IoBitReader::endian(Cursor::new(&bytes[jm_pos + 4..]), LittleEndian);
        let _ = reader.skip(bit_pos as u32);
        let mut cursor = BitCursor::new(reader);
        if let Ok(_) = Item::from_reader_with_context(&mut cursor, &huffman, Some((&bytes, start_bit + bit_pos)), is_alpha, current_idx == 0, None) {
            bit_pos += cursor.pos();
        } else {
            break;
        }
    }
}

fn print_item_segments<R: BitRead>(cursor: &BitCursor<R>) {
    let mut segments = cursor.segments().to_vec();
    segments.sort_by_key(|s| s.start);
    for seg in segments {
        let indent = "  ".repeat(seg.depth + 1);
        println!("{} [{:>4}..{:>4}] (len={:>3}) {}", indent, seg.start, seg.end, seg.end - seg.start, seg.label);
    }
}
