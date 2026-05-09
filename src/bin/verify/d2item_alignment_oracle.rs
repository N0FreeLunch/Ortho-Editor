use bitstream_io::{BitRead, BitReader, LittleEndian};
use d2r_core::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header};
use d2r_core::verify::args::{ArgParser, ArgSpec};
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Cursor;

#[derive(Serialize)]
struct AlignmentReport {
    total_markers: usize,
    intervals: Vec<u64>,
    distribution: HashMap<u64, usize>,
    contextual_mapping: HashMap<String, Vec<u64>>,
    inferred_alignment: Option<u64>,
}

fn main() {
    let mut parser = ArgParser::new("d2item_alignment_oracle");
    parser.add_spec(ArgSpec::positional("save_file", "Path to save file"));
    parser.add_spec(
        ArgSpec::positional("offset", "Bit offset from start to begin search")
            .optional()
            .with_default("0"),
    );
    parser.add_spec(ArgSpec::flag("json", None, Some("json"), "Output results in JSON format"));
    parser.add_spec(ArgSpec::flag("detect-desync", None, Some("detect-desync"), "Run desync detector against found markers"));

    use d2r_core::verify::args::ArgError;
    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
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

    let path = parsed.get("save_file").unwrap();
    let offset = parsed
        .get("offset")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let use_json = parsed.is_set("json");
    let do_detect_desync = parsed.is_set("detect-desync");

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            std::process::exit(1);
        }
    };

    let jm_pos = match (0..bytes.len() - 2).find(|&i| bytes[i] == b'J' && bytes[i + 1] == b'M') {
        Some(pos) => pos,
        None => {
            eprintln!("No initial JM marker found (base search anchor).");
            std::process::exit(1);
        }
    };

    let huffman = HuffmanTree::new();
    let is_alpha = bytes[4..8] == [0x69, 0, 0, 0];

    if !use_json {
        println!("Starting exhaustive bit-level JM search from byte {} (bit {})...", jm_pos, jm_pos * 8);
        println!("Marker Pattern: 0x4D4A ('JM' in LittleEndian)");
        println!("Mode: {}", if is_alpha { "Alpha v105" } else { "Retail" });
        println!("{:-<75}", "");
        println!("{:>12} | {:>10} | {:>10} | {:>15}", "Bit Offset", "Abs Byte", "Interval", "Item Code");
        println!("{:-<75}", "");
    }

    let start_bit = (jm_pos * 8) as u64 + offset;
    let total_bits = (bytes.len() * 8) as u64;
    
    let mut last_marker_pos = (jm_pos * 8) as u64 + 32;
    let mut found_count = 0;
    let mut intervals = Vec::new();
    let mut contextual_mapping: HashMap<String, Vec<u64>> = HashMap::new();

    let mut bit_idx = last_marker_pos;
    while bit_idx < total_bits - 100 {
        if let Some((mode, location, _, code, flags, version, _is_compact, _header_bits, _nudge, _has_checksum)) =
            peek_item_header_at(&bytes, bit_idx, &huffman, is_alpha)
        {
            if is_plausible_item_header(mode, location, &code, flags, version, is_alpha) {
                // Heuristic: v105 items should be version 5 or similar
                if !is_alpha || version == 5 {
                    if found_count > 0 {
                        let interval = bit_idx - last_marker_pos;
                        intervals.push(interval);
                        contextual_mapping.entry(code.trim().to_string()).or_default().push(interval);
                    }
                    
                    let trimmed_code = code.trim().to_string();
                    if !use_json {
                        println!("{:12} | {:10.2} | {:>15} | v={}", bit_idx, bit_idx as f64 / 8.0, trimmed_code, version);
                    }
                    
                    last_marker_pos = bit_idx;
                    found_count += 1;
                    
                    // Jump ahead by at least 72 bits (minimum item size)
                    bit_idx += 72;
                    continue;
                }
            }
        }
        bit_idx += 8; // Byte-aligned search
    }

    let mut distribution = HashMap::new();
    for &val in &intervals {
        *distribution.entry(val).or_insert(0) += 1;
    }

    let inferred_alignment = distribution.iter()
        .max_by_key(|&(_, count)| count)
        .map(|(&val, _)| val);

    if use_json {
        let report = AlignmentReport {
            total_markers: found_count,
            intervals,
            distribution,
            contextual_mapping,
            inferred_alignment,
        };
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("{:-<75}", "");
        println!("Search complete. Found {} markers.", found_count);
        if let Some(align) = inferred_alignment {
            println!("Inferred Alignment Rule: {} bits (most frequent interval)", align);
        }
        
        println!("\nContextual Mapping (Item Code -> Observed Intervals):");
        let mut sorted_keys: Vec<_> = contextual_mapping.keys().collect();
        sorted_keys.sort();
        for key in sorted_keys {
            println!("  {:<10}: {:?}", key, contextual_mapping[key]);
        }

        if do_detect_desync {
            println!("\n{:-<80}", "");
            println!("Running Desync Detector...");
            match d2r_core::verify::desync::detect_desync(&bytes, &huffman, is_alpha) {
                Ok(reports) => {
                    println!("{:>5} | {:>12} | {:>12} | {:>8} | {:>5}", "Idx", "Oracle", "Parser", "Drift", "Match");
                    for r in reports {
                        println!("{:5} | {:12} | {:12} | {:8} | {:5}", 
                            r.item_index, r.oracle_start, r.parser_start, r.drift, if r.is_match { "OK" } else { "FAIL" }
                        );
                    }
                }
                Err(e) => println!("Desync detector failed: {}", e),
            }
        }
    }
}
