use anyhow::{Context, Result};
use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::env;
use std::fs;
use std::io::Cursor;

use d2r_core::verify::args::{ArgParser, ArgSpec};

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
    let mut parser = ArgParser::new("d2item_geometry_scanner");
    parser.add_spec(ArgSpec::option("path", None, Some("path"), "Path to the d2s file").required());
    parser.add_spec(ArgSpec::option("bit", None, Some("bit"), "Start bit of the item").required());
    parser.add_spec(ArgSpec::flag(
        "alpha",
        None,
        Some("alpha"),
        "Force Alpha mode (18-bit properties)",
    ));

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

    let path = parsed.get("path").cloned().unwrap();
    let start_bit: u64 = parsed
        .get("bit")
        .cloned()
        .unwrap()
        .parse()
        .context("Invalid bit offset")?;
    let force_alpha = parsed.is_set("alpha");

    let bytes = fs::read(&path).with_context(|| format!("Failed to read file: {}", path))?;

    println!("--- D2 Item Geometry Scanner MVP ---");
    println!("File: {}", path);
    println!("Item Start Bit: {}", start_bit);

    let id_bits = 9;
    let terminator_id = 511;

    // 1. Search for terminator candidates
    let mut terminator_candidates = Vec::new();
    let scan_limit = 3000;

    for i in 0..scan_limit {
        let bit_pos = start_bit + i;
        let byte_offset = (bit_pos / 8) as usize;
        let bit_offset = (bit_pos % 8) as u32;

        if byte_offset + 3 >= bytes.len() {
            break;
        }

        let mut reader = BitReader::endian(Cursor::new(&bytes[byte_offset..]), LittleEndian);
        for _ in 0..bit_offset {
            let _ = reader.read_bit();
        }

        let id = read_bits(&mut reader, id_bits);
        if id == terminator_id {
            // Check for the extra bit (common in Version 5 Alpha items)
            let extra = reader.read_bit().unwrap_or(false);
            terminator_candidates.push((i, extra));
        }
    }

    println!(
        "Found {} terminator candidates.",
        terminator_candidates.len()
    );

    // 2. Simulation Logic
    let mut results = Vec::new();
    let huffman = d2r_core::item::HuffmanTree::new();

    for &(t_rel, extra) in &terminator_candidates {
        // H: Header lengths [40..130]
        for h in 40..130 {
            if t_rel < h {
                continue;
            }
            let remainder = t_rel - h;

            let is_alpha_fit = remainder > 0 && remainder % 18 == 0;
            let is_retail_fit = remainder > 0 && remainder % 14 == 0;

            // Try decoding code
            let mut code = String::new();
            let mut h_reader = BitReader::endian(
                Cursor::new(&bytes[(start_bit / 8) as usize..]),
                LittleEndian,
            );
            let skip_bits = (start_bit % 8) + h as u64;
            for _ in 0..skip_bits {
                let _ = h_reader.read_bit();
            }
            let mut h_cursor = d2r_core::data::bit_cursor::BitCursor::new(h_reader);

            for _ in 0..4 {
                if let Ok(ch) = huffman.decode_recorded(&mut h_cursor) {
                    code.push(ch);
                }
            }

            let mut score = 0;
            if is_alpha_fit {
                score += 100;
            }

            // Code plausibility boost
            if code.len() == 4 && code.chars().all(|c| c.is_alphanumeric() || c == ' ') {
                if code.trim().len() >= 3 {
                    score += 500;
                }
            }

            if is_alpha_fit && extra {
                score += 10;
            }

            if score > 500 {
                results.push((t_rel, h, remainder, score, extra, code));
            }
        }
    }

    if force_alpha {
        results.retain(|r| r.2 % 18 == 0);
    }

    // Rank results
    results.sort_by(|a, b| {
        if b.3 != a.3 {
            b.3.cmp(&a.3)
        } else {
            // Tie-break with distance to common header lengths (approx 76-85)
            let dist_a = (a.1 as i32 - 80).abs();
            let dist_b = (b.1 as i32 - 80).abs();
            dist_a.cmp(&dist_b)
        }
    });

    // Rank results
    results.sort_by(|a, b| b.3.cmp(&a.3));

    println!(
        "\nRanked Geometry Candidates (Relative to Start Bit {}):",
        start_bit
    );
    println!(
        "{:<10} {:<10} {:<10} {:<15} {:<10} {:<10} {:<5}",
        "Term(Rel)", "Header", "PropsBits", "Rhythm", "Code", "Score", "Extra"
    );

    for (t, h, r, score, extra, code) in results.iter().take(40) {
        let rhythm = if r % 18 == 0 {
            "18 (Alpha)"
        } else if r % 14 == 0 {
            "14 (Retail?)"
        } else {
            "Unknown"
        };
        println!(
            "{:<10} {:<10} {:<10} {:<15} {:<10} {:<10} {:<5}",
            t, h, r, rhythm, code, score, extra
        );
    }

    if results.is_empty() {
        println!("\nNo clear geometry candidates found.");
    }

    Ok(())
}
