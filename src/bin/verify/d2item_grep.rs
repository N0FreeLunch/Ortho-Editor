use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde_json::json;
use std::env;
use std::fs;
use std::io::Cursor;

fn main() {
    let mut parser = ArgParser::new("d2item_grep").description(
        "Searches a .d2s or .d2i file for specific bit patterns or semantic properties.",
    );
    parser.add_spec(ArgSpec::positional("file", "Path to .d2i or .d2s file"));
    parser.add_spec(ArgSpec::option(
        "pattern",
        Some('p'),
        Some("pattern"),
        "Raw bit pattern to search for (e.g., '1010111000')",
    ));
    parser.add_spec(ArgSpec::flag(
        "json",
        None,
        Some("json"),
        "Output results in JSON format",
    ));

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
    let is_json = parsed.is_json();
    let pattern_str = parsed.get("pattern");

    if pattern_str.is_none() {
        if is_json {
            println!(
                "{}",
                json!({"errors": ["No search pattern provided. Use --pattern."]})
            );
        } else {
            eprintln!("error: No search pattern provided. Use --pattern.");
        }
        return;
    }

    let pattern_str = pattern_str.unwrap();
    let pattern_bits: Vec<bool> = pattern_str
        .chars()
        .filter_map(|c| match c {
            '1' => Some(true),
            '0' => Some(false),
            _ => None,
        })
        .collect();

    if pattern_bits.is_empty() {
        if is_json {
            println!(
                "{}",
                json!({"errors": ["Invalid pattern. Must contain '0' and '1'."]})
            );
        } else {
            eprintln!("error: Invalid pattern. Must contain '0' and '1'.");
        }
        return;
    }

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            if is_json {
                println!(
                    "{}",
                    json!({"errors": [format!("Failed to read file: {}", e)]})
                );
            } else {
                eprintln!("Failed to read file: {}", e);
            }
            return;
        }
    };

    let mut matches = Vec::new();
    let mut reader = BitReader::endian(Cursor::new(&bytes), LittleEndian);

    // Naive bit-by-bit search
    let total_bits = (bytes.len() * 8) as u64;
    let mut current_pos = 0;

    // We can't rewind the bitreader easily, so we'll buffer the last N bits
    let mut window = Vec::with_capacity(pattern_bits.len());

    while current_pos < total_bits {
        if let Ok(bit) = reader.read_bit() {
            window.push(bit);
            if window.len() > pattern_bits.len() {
                window.remove(0);
            }

            if window.len() == pattern_bits.len() && window == pattern_bits {
                let match_start = current_pos + 1 - pattern_bits.len() as u64;
                matches.push(match_start);
            }
        } else {
            break;
        }
        current_pos += 1;
    }

    if is_json {
        println!(
            "{}",
            json!({
                "pattern": pattern_str,
                "matches_count": matches.len(),
                "matches": matches,
                "errors": []
            })
        );
    } else {
        println!(
            "Found {} matches for pattern '{}'",
            matches.len(),
            pattern_str
        );
        for m in matches.iter().take(100) {
            println!("  Match at offset: {}", m);
        }
        if matches.len() > 100 {
            println!("  ... and {} more matches", matches.len() - 100);
        }
    }
}
