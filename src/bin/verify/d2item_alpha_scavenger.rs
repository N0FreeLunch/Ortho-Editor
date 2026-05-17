use bitstream_io::{BitReader as IoBitReader, LittleEndian};
use d2r_core::data::bit_cursor::BitCursor;
use d2r_core::item::HuffmanTree;
use std::env;
use std::fs;
use std::io::Cursor;
use std::process;
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};

fn main() {
    let mut parser = ArgParser::new("d2item_alpha_scavenger")
        .description("Scavenges for plausible Alpha v105 item headers across the entire save file bitstream");

    parser.add_spec(ArgSpec::positional("save_file", "path to the Alpha v105 save file (.d2s)"));

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

    let huffman = HuffmanTree::new();
    let is_alpha = bytes.len() > 8 && bytes[4..8] == [0x69, 0, 0, 0];
    if !is_alpha {
        println!("Not an Alpha v105 save file.");
        return;
    }

    let jm_pos = (0..bytes.len().saturating_sub(1))
        .find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M')
        .expect("No JM marker found");

    let start_bit = (jm_pos + 4) * 8;
    let end_bit = bytes.len() * 8;

    println!("Scavenging for Alpha items from bit {} to {}:", start_bit, end_bit);

    for bit in start_bit as u64..end_bit as u64 {
        let byte_idx = (bit / 8) as usize;
        if byte_idx >= bytes.len() { break; }

        let reader = IoBitReader::endian(Cursor::new(&bytes[byte_idx..]), LittleEndian);
        let mut cursor = BitCursor::new(reader);
        let _ = cursor.skip_and_record((bit % 8) as u32);

        let checkpoint = cursor.pos();
        let flags = match cursor.read_bits::<u32>(32) { Ok(v) => v, _ => continue };
        let version = match cursor.read_bits::<u32>(3) { Ok(v) => v, _ => continue };
        let mode = match cursor.read_bits::<u32>(3) { Ok(v) => v, _ => continue };
        let loc = match cursor.read_bits::<u32>(3) { Ok(v) => v, _ => continue };
        let x = match cursor.read_bits::<u32>(4) { Ok(v) => v, _ => continue };

        let header_bits_before_gap = cursor.pos() - checkpoint;

        for gap in 0..=16u64 {
            let total_offset = bit + header_bits_before_gap + gap;
            let gap_byte_idx = (total_offset / 8) as usize;
            if gap_byte_idx >= bytes.len() { continue; }

            let g_reader = IoBitReader::endian(Cursor::new(&bytes[gap_byte_idx..]), LittleEndian);
            let mut g_cursor = BitCursor::new(g_reader);
            let _ = g_cursor.skip_and_record((total_offset % 8) as u32);

            let mut code = String::new();
            let mut fail = false;
            for _ in 0..4 {
                match huffman.decode_recorded(&mut g_cursor) {
                    Ok(ch) => code.push(ch),
                    _ => { fail = true; break; }
                }
            }

            let axiom = d2r_core::domain::header::entity::HeaderAxiom {
                version: version as u8,
                alpha_mode: true,
            };

            if !fail && axiom.is_plausible(mode as u8, loc as u8, code.as_bytes(), flags) {
                println!("  [Bit {}] Potential Item: '{}' (mode={}, loc={}, x={}, flags=0x{:08X}, gap={})",     
                    bit, code, mode, loc, x, flags, gap);
            }
        }
    }
}
