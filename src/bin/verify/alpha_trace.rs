use anyhow::Context;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser};
use d2r_core::verify::OutputManager;
use std::{env, fs};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2item_alpha_trace")
        .description("Traces Alpha v105 item parsing with optional bit offset support.");
    parser
        .add_opt("file", "Path to save file")
        .long("file")
        .short('f');
    parser
        .add_opt("offset", "Starting bit offset for tracing")
        .long("offset")
        .short('o');
    parser
        .add_opt("len", "Number of bits to trace (not yet fully supported by high-level parser)")
        .long("len")
        .short('l');
    parser
        .add_flag("alpha", "Force Alpha v105 mode")
        .long("alpha")
        .short('a');

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            eprintln!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => anyhow::bail!("error: {}\n\n{}", e, parser.usage()),
    };

    let mut om = OutputManager::new("d2item_alpha_trace", &parsed);

    let file_path = parsed.get("file").context("Missing --file argument")?;
    let offset: Option<u64> = parsed.get("offset").and_then(|s| s.parse().ok());
    let _len: Option<u64> = parsed.get("len").and_then(|s| s.parse().ok());
    let force_alpha = parsed.is_set("alpha");

    let bytes = fs::read(file_path).with_context(|| format!("Failed to read file: {}", file_path))?;
    let huffman = HuffmanTree::new();

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let is_alpha = force_alpha || version == 105;

    om.summary(&format!("Tracing items in {} (alpha={})", file_path, is_alpha));

    if let Some(start_bit) = offset {
        om.summary(&format!("Targeted trace starting at bit offset {}", start_bit));
    }

    match Item::read_player_items(&bytes, &huffman, is_alpha) {
        Ok(items) => {
            let mut matched_items = Vec::new();
            for item in &items {
                if let Some(start_bit) = offset {
                    if item.range.start >= start_bit {
                        matched_items.push(item);
                    }
                } else {
                    matched_items.push(item);
                }
            }

            om.println(&format!("Parsed {} items (showing {}).", items.len(), matched_items.len()));
            for (i, item) in matched_items.iter().enumerate() {
                let trimmed = d2r_core::item::normalize_alpha_code_hint(item.code.trim());
                let h_axiom = d2r_core::domain::header::entity::HeaderAxiom::new(item.header.version, is_alpha);
                let is_rw = h_axiom.is_runeword(item.header.flags, Some(&item.code), item.header.has_checksum);
                om.println(&format!(
                    "Item {}: code={}, start={}, bin_len={} bits, is_rw={}",
                    i,
                    trimmed,
                    item.range.start,
                    item.bits.len(),
                    is_rw
                ));
                om.println(&format!(
                    "  Header: flags=0x{:08X}, v={}, m={}, l={}, x={}, has_checksum={}",
                    item.header.flags,
                    item.header.version,
                    item.header.mode,
                    item.header.location,
                    item.header.x,
                    item.header.has_checksum
                ));
                if !item.properties.is_empty() {
                    om.println("  Properties:");
                    for prop in &item.properties {
                        om.println(&format!("    - id={:3}, val={}", prop.stat_id, prop.value));
                    }
                }
                if !item.socketed_items.is_empty() {
                    om.println(&format!("  Socketed items ({}):", item.socketed_items.len()));
                    for (si, child) in item.socketed_items.iter().enumerate() {
                        let c_trimmed = d2r_core::item::normalize_alpha_code_hint(child.code.trim());
                        om.println(&format!("    [{}] code={}, len={} bits", si, c_trimmed, child.bits.len()));
                    }
                }
            }
        }
        Err(e) => {
            om.println(&format!("Error parsing items: {}", e));
        }
    }

    Ok(())
}
