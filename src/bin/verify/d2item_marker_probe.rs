use std::env;
use std::fs;
use anyhow::{Result, Context};
use d2r_core::item::{HuffmanTree, peek_item_header_at, peek_item_header_at_specific_gap, is_plausible_item_header};
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_marker_probe")
        .description("Single-offset forensic probe for item marker plausibility");
    
    parser.add_spec(ArgSpec::option("file", Some('f'), Some("file"), "Path to D2S save file").required());
    parser.add_spec(ArgSpec::option("offset", Some('o'), Some("offset"), "Bit offset to probe").required());
    parser.add_spec(ArgSpec::flag("alpha", Some('a'), Some("alpha"), "Enable Alpha v105 mode"));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let file_path = parsed.get("file").unwrap();
    let offset_str = parsed.get("offset").unwrap();
    let offset: u64 = offset_str.parse().with_context(|| format!("Invalid bit offset: {}", offset_str))?;
    let alpha_mode = parsed.is_set("alpha");

    let bytes = fs::read(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;
    
    let huffman = HuffmanTree::new();

    println!("Probing file: {}", file_path);
    println!("Bit offset: {}", offset);
    println!("Alpha mode: {}", alpha_mode);
    println!("{:-<40}", "");

    let mut found = false;
    // Iterate through some reasonable gaps to see multiple candidates
    for gap in 0..64 {
        if let Some((mode, location, _x, code, flags, version, _is_compact, _header_bits, nudge_val, _has_checksum)) =
            peek_item_header_at_specific_gap(&bytes, offset, &huffman, alpha_mode, gap as u64)
        {
            if is_plausible_item_header(mode, location, code.as_bytes(), flags, version, alpha_mode) {
                println!("Candidate at bit {} (Gap {}):", offset, gap);
                println!("  Flags:    0x{:08X}", flags);
                println!("  Version:  {}", version);
                println!("  Code:     '{}'", &code);
                println!("  Nudge:    {}", nudge_val);
                println!("{:-<20}", "");
                found = true;
            }
        }
    }
    
    if !found {
        println!("Verdict: [EXTRACTION FAILURE / NO PLAUSIBLE ITEM AT OFFSET]");
    }

    Ok(())
}

fn mode_name(mode: u8) -> &'static str {
    match mode {
        0 => "Stored",
        1 => "Equipped",
        2 => "Belt",
        4 => "Cursor",
        6 => "Socketed",
        _ => "Unknown",
    }
}

fn location_name(loc: u8) -> &'static str {
    match loc {
        0 => "None",
        1 => "Inventory",
        4 => "Stash",
        5 => "Cube",
        _ => "Other",
    }
}
