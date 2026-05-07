use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::save::{gf_payload_range, map_core_sections};
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};
use std::env;
use std::fs;
use std::io::{self, Cursor};

fn get_retail_bits(stat_id: u32) -> u32 {
    match stat_id {
        0 | 1 | 2 | 3 | 4 => 10,
        5 => 8,
        6 | 7 | 8 | 9 | 10 | 11 => 21,
        12 => 7,
        13 => 32,
        14 | 15 => 25,
        85 => 8,
        _ => 0, // Unknown
    }
}

fn parse_range(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split("..").collect();
    if parts.len() == 2 {
        let start = parts[0].parse().unwrap_or(0);
        let end_part = parts[1].trim_start_matches('=');
        let end = end_part.parse().unwrap_or(16);
        Some((start, end))
    } else {
        None
    }
}

fn main() -> io::Result<()> {
    let mut parser = ArgParser::new("d2gf_oracle")
        .description("Value-bit width sweep oracle for discovering unknown stat bit widths in GF sections");

    parser.add_spec(ArgSpec::positional("save_path", "path to the D2R save file (.d2s)"));
    parser.add_spec(ArgSpec::option("stat-id", Some('i'), Some("stat-id"), "target stat ID to oracle (default: 80)").with_default("80"));
    parser.add_spec(ArgSpec::option("width-range", Some('w'), Some("width-range"), "value-bit width range to sweep (default: 0..16)").with_default("0..16"));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            std::process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}\n\n{}", e, parser.usage());
            std::process::exit(1);
        }
    };

    let path = parsed.get("save_path").unwrap();
    let target_id: u32 = parsed.get("stat-id").and_then(|s| s.parse().ok()).unwrap_or(80);
    
    let (start_width, end_width) = parsed.get("width-range")
        .and_then(|s| parse_range(s))
        .unwrap_or((0, 16));

    let bytes = fs::read(path)?;

    let map =
        map_core_sections(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let payload_range = gf_payload_range(&map);
    let payload = &bytes[payload_range.start..payload_range.end];

    println!("=== GF Oracle Sweep ===");
    println!("File      : {}", path);
    println!("Target ID : {}", target_id);
    println!(
        "Payload   : {} bytes (at 0x{:X})",
        payload.len(),
        payload_range.start
    );
    println!();
    println!("| Val Width | Score | Terminator Found | Terminator Bit Offset | Notes |");
    println!("|-----------|-------|------------------|-----------------------|-------|");

    for val_width in start_width..=end_width {
        let (score, term_found, term_pos, notes) = score_width(payload, target_id, val_width);
        println!(
            "| {:9} | {:5} | {:16} | {:21} | {:5} |",
            val_width,
            score,
            term_found,
            term_pos.map(|p| p.to_string()).unwrap_or("-".to_string()),
            notes
        );
    }

    Ok(())
}

fn score_width(payload: &[u8], target_id: u32, val_width: u32) -> (i32, bool, Option<u64>, String) {
    let mut reader = BitReader::endian(Cursor::new(payload), LittleEndian);
    let mut score = 0;
    let mut term_found = false;
    let mut term_pos = None;
    let mut notes = String::new();
    let mut stats_read = 0;

    loop {
        let bit_pos = reader.position_in_bits().unwrap_or(0);
        let stat_id = match reader.read::<9, u32>() {
            Ok(id) => id,
            Err(_) => {
                notes = "EOF".to_string();
                break;
            }
        };

        if stat_id == 0x1FF {
            term_found = true;
            term_pos = Some(bit_pos);
            score += 1000;
            break;
        }

        stats_read += 1;
        if stats_read > 100 {
            notes = "Too many stats".to_string();
            break;
        }

        if stat_id == target_id {
            // Use candidate width
            if reader.skip(val_width).is_err() {
                notes = "Skip fail (target)".to_string();
                score -= 500;
                break;
            }
            score += 100;
        } else {
            let retail_bits = get_retail_bits(stat_id);
            if retail_bits > 0 {
                if reader.skip(retail_bits).is_err() {
                    notes = "Skip fail (retail)".to_string();
                    score -= 500;
                    break;
                }
                score += 50;
            } else {
                // Unknown stat ID that is not our target
                notes = format!("Unknown ID {}", stat_id);
                score -= 1000; // Desync penalty
                break;
            }
        }
    }

    if !term_found {
        score -= 2000;
    }

    (score, term_found, term_pos, notes)
}
