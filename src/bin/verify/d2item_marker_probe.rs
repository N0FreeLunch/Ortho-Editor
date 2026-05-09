use std::env;
use std::fs;
use anyhow::{Result, Context};
use d2r_core::item::{HuffmanTree, peek_item_header_at, is_plausible_item_header};
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

    if let Some((mode, location, x, code, flags, version, is_compact, header_bits, _nudge, _has_checksum)) =
        peek_item_header_at(&bytes, offset, &huffman, alpha_mode)
    {
        println!("Header found at bit {}:", offset);
        println!("  Flags:    0x{:08X}", flags);
        println!("  Version:  {}", version);
        println!("  Mode:     {} (0x{:X})", mode_name(mode), mode);
        println!("  Location: {} (0x{:X})", location_name(location), location);
        println!("  X Coord:  {}", x);
        println!("  Code:     '{}'", code);
        println!("  Compact:  {}", is_compact);
        println!("  Hdr bits: {}", header_bits);

        let plausible = is_plausible_item_header(mode, location, &code, flags, version, alpha_mode);
        if plausible {
            println!("\nVerdict: [REAL CANDIDATE]");
        } else {
            println!("\nVerdict: [IMPLAUSIBLE BUT DECODABLE]");
        }
    } else {
        println!("Verdict: [EXTRACTION FAILURE / NO ITEM AT OFFSET]");
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
