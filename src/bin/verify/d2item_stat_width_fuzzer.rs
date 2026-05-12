use d2r_core::item::HuffmanTree;
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};
use bitstream_io::{BitRead, BitReader, LittleEndian};
use std::env;
use std::fs;
use std::io::Cursor;
use std::process;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = ArgParser::new("d2item_stat_width_fuzzer")
        .description("Brute-force bit-widths at a specific offset to find the correct alignment");
    
    parser.add_spec(ArgSpec::option("file", None, Some("file"), "Path to the save file").required());
    parser.add_spec(ArgSpec::option("offset", None, Some("offset"), "Bit offset to start fuzzing from").required());
    parser.add_spec(ArgSpec::flag("json", None, Some("json"), "Output results in JSON format"));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let file_path = parsed.get("file").unwrap();
    let start_offset: u64 = parsed.get("offset").unwrap().parse().expect("offset must be a number");
    let use_json = parsed.is_set("json");

    let bytes = fs::read(file_path)?;
    let _huffman = HuffmanTree::new();

    if !use_json {
        println!("Fuzzing bit-widths (1..32) at offset {} in {}", start_offset, file_path);
        println!("{:-<60}", "");
        println!("{:>5} | {:>15} | {:>10} | {}", "Width", "Next JM Offset", "Alignment", "Status");
        println!("{:-<60}", "");
    }

    let mut results = Vec::new();

    for width in 1..=32 {
        let current_pos = start_offset + width as u64;
        let mut found_jm = None;
        
        // Scan for next JM marker (0x4A 0x4D)
        // We look ahead up to 1024 bits
        for i in 0..1024 {
            let probe_pos = current_pos + i;
            if probe_pos + 16 > (bytes.len() * 8) as u64 { break; }
            
            // Manual byte alignment check for JM
            if probe_pos % 8 == 0 {
                let byte_idx = (probe_pos / 8) as usize;
                if byte_idx + 1 < bytes.len() && bytes[byte_idx] == b'J' && bytes[byte_idx+1] == b'M' {
                    found_jm = Some(probe_pos);
                    break;
                }
            }
        }

        let alignment = found_jm.map(|pos| pos % 8).unwrap_or(99);
        let status = if alignment == 0 { "CANDIDATE" } else { "" };
        
        if use_json {
            results.push(serde_json::json!({
                "width": width,
                "next_jm": found_jm,
                "alignment_error": alignment,
                "is_candidate": alignment == 0
            }));
        } else if alignment == 0 || width == 12 { // Always show 12 for runeword debugging
             println!("{:>5} | {:>15?} | {:>10} | {}", width, found_jm, alignment, status);
        }
    }

    if use_json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        println!("{:-<60}", "");
    }

    Ok(())
}
