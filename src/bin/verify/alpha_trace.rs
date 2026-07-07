use anyhow::{Context, Result};
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgParser, ArgSpec};
use std::env;
use std::fs;

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_alpha_trace");
    parser.add_spec(ArgSpec::option(
        "file",
        Some('f'),
        Some("file"),
        "Path to the savegame file (.d2s)",
    ));
    parser.add_spec(ArgSpec::option(
        "offset",
        Some('o'),
        Some("offset"),
        "Bit offset to start parsing from (default: 0)",
    ));
    parser.add_spec(ArgSpec::flag(
        "alpha",
        Some('a'),
        Some("alpha"),
        "Enable Alpha v105 parsing rules",
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

    let file_path = parsed
        .get("file")
        .cloned()
        .context("File path required (--file)")?;
    let offset_str = parsed
        .get("offset")
        .cloned()
        .unwrap_or_else(|| "0".to_string());
    let offset: u64 = offset_str.parse().context("Invalid offset")?;
    let alpha = parsed.is_set("alpha");

    // Force tracing on
    unsafe {
        env::set_var("D2R_ITEM_TRACE", "1");
    }

    let bytes =
        fs::read(&file_path).with_context(|| format!("Failed to read file: {}", file_path))?;

    let huffman = HuffmanTree::new();

    println!("Tracing items in {} (alpha={})", file_path, alpha);
    if offset > 0 {
        println!("Targeted trace starting at bit offset {}", offset);
    }

    // If offset is provided, we try to parse a single item at that offset.
    // Otherwise, we parse all player items.
    if offset > 0 {
        match Item::parse_at_bit_offset(&bytes, offset, &huffman, alpha) {
            Ok(item) => {
                print_item_trace(0, &item);
            }
            Err(e) => {
                println!("Error parsing item at offset {}: {}", offset, e);
            }
        }
    } else {
        match Item::read_player_items(&bytes, &huffman, alpha) {
            Ok(items) => {
                println!("Parsed {} items.", items.len());
                for (i, item) in items.iter().enumerate() {
                    print_item_trace(i, item);
                }
            }
            Err(e) => {
                println!("Error parsing items: {}", e);
            }
        }
    }

    Ok(())
}

fn print_item_trace(idx: usize, item: &Item) {
    println!(
        "Item {}: code={}, start={}, bin_len={} bits, is_rw={}",
        idx,
        item.code,
        item.range.start,
        item.range.end - item.range.start,
        item.header.is_runeword
    );
    println!(
        "  Header: flags=0x{:08X}, v={}, m={}, l={}, x={}, has_checksum={}",
        item.header.flags,
        item.header.version,
        item.header.mode,
        item.header.location,
        item.header.x,
        item.header.has_checksum
    );

    if !item.stats.properties.is_empty() {
        println!("  Properties ({}):", item.stats.properties.len());
        for prop in &item.stats.properties {
            println!("    - id={:>3}, val={}", prop.stat_id, prop.value);
        }
    }

    if !item.socketed_items.is_empty() {
        println!("  Socketed items ({}):", item.socketed_items.len());
        for (si, s_item) in item.socketed_items.iter().enumerate() {
            println!(
                "    [{}] code={}, len={} bits",
                si,
                s_item.code,
                s_item.range.end - s_item.range.start
            );
        }
    }
}
