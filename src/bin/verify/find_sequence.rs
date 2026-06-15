use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::fs;
use std::io::Cursor;
use anyhow::{Context, Result};
use d2r_core::verify::args::{ArgParser, ArgSpec};
use std::env;

fn read_bits<R: BitRead>(reader: &mut R, n: u32) -> u32 {
    let mut value = 0u32;
    for i in 0..n {
        if let Ok(b) = reader.read_bit() {
            if b {
                value |= 1 << i;
            }
        }
    }
    value
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_find_sequence");
    parser.add_spec(ArgSpec::option("file", Some('f'), Some("file"), "Path to the savegame file (.d2s)"));
    parser.add_spec(ArgSpec::option("offset", Some('o'), Some("offset"), "Bit offset to start searching from"));

    let args: Vec<_> = env::args_os().skip(1).collect();
    use d2r_core::verify::args::ArgError;
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            std::process::exit(1);
        }
    };

    let file_path = parsed.get("file").cloned().context("File path required (--file)")?;
    let offset_str = parsed.get("offset").cloned().context("Offset required (--offset)")?;
    let start_bit: u64 = offset_str.parse().context("Invalid offset")?;

    let bytes = fs::read(&file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    println!(
        "--- Alpha v105 Property Brute Force (Start: {}) ---",
        start_bit
    );

    // Common Alpha v105 Stat IDs (raw)
    let targets = [8, 310, 114, 287, 289, 72, 73, 127, 256, 496, 499, 31, 26];

    for id_bits in [9, 10, 11] {
        for v_bits in 1..=20 {
            let byte_offset = start_bit / 8;
            let bit_offset = start_bit % 8;
            let mut reader =
                BitReader::endian(Cursor::new(&bytes[byte_offset as usize..]), LittleEndian);
            for _ in 0..bit_offset {
                let _ = reader.read_bit().ok();
            }

            let mut found = Vec::new();
            let mut sequence = Vec::new();

            for _ in 0..100 {
                let id = read_bits(&mut reader, id_bits);
                if id == (1 << id_bits) - 1 {
                    sequence.push((id, 0));
                    break;
                }
                let val = read_bits(&mut reader, v_bits);
                sequence.push((id, val));
                if targets.contains(&(id as u16)) {
                    found.push(id);
                }
            }

            if found.len() >= 1 {
                println!(
                    "Hit! (ID bits: {}, Val bits: {}) Found IDs: {:?} Sequence: {:?}",
                    id_bits, v_bits, found, sequence
                );
            }
        }
    }
    
    Ok(())
}
