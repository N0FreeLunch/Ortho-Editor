use d2r_core::algo::alignment::BitAligner;
use d2r_core::item::{HuffmanTree, Item, ItemProperty};
use d2r_core::verify::args::{ArgParser, ArgSpec, ArgError};
use std::env;
use std::fs;
use std::path::Path;
use std::process;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = ArgParser::new("d2item_bit_diff")
        .description("Semantic and bit-level differential auditor for items");
    
    parser.add_spec(ArgSpec::option("save1", None, Some("save1"), "Path to first save file").required());
    parser.add_spec(ArgSpec::option("index1", None, Some("index1"), "Item index in first save file").required());
    parser.add_spec(ArgSpec::option("save2", None, Some("save2"), "Path to second save file").required());
    parser.add_spec(ArgSpec::option("index2", None, Some("index2"), "Item index in second save file").required());
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

    let save1_path = parsed.get("save1").unwrap();
    let index1: usize = parsed.get("index1").unwrap().parse().expect("index1 must be a number");
    let save2_path = parsed.get("save2").unwrap();
    let index2: usize = parsed.get("index2").unwrap().parse().expect("index2 must be a number");
    let use_json = parsed.is_set("json");

    let bytes1 = fs::read(save1_path)?;
    let bytes2 = fs::read(save2_path)?;

    let huffman = HuffmanTree::new();

    let items1 = load_items(&bytes1, &huffman, save1_path);
    let items2 = load_items(&bytes2, &huffman, save2_path);

    if index1 >= items1.len() {
        eprintln!(
            "Error: Item index {} out of range for {} (found {} items)",
            index1,
            save1_path,
            items1.len()
        );
        process::exit(1);
    }
    if index2 >= items2.len() {
        eprintln!(
            "Error: Item index {} out of range for {} (found {} items)",
            index2,
            save2_path,
            items2.len()
        );
        process::exit(1);
    }

    let item1 = &items1[index1];
    let item2 = &items2[index2];

    if !use_json {
        println!("--- Bitstream Alignment Diff ---");
        println!(
            "Item A: {} #{} ({})",
            Path::new(save1_path).file_name().unwrap_or_default().to_string_lossy(),
            index1,
            item1.code.trim()
        );
        println!(
            "Item B: {} #{} ({})",
            Path::new(save2_path).file_name().unwrap_or_default().to_string_lossy(),
            index2,
            item2.code.trim()
        );
        println!("--------------------------------");
    }

    let bits1: Vec<bool> = item1.bits.iter().map(|rb| rb.bit).collect();
    let bits2: Vec<bool> = item2.bits.iter().map(|rb| rb.bit).collect();

    let aligner = BitAligner::new(2, -1, -3, -1); // match, mismatch, gap_open, gap_extend
    let result = aligner.align(&bits1, &bits2);

    if !use_json {
        println!("Score        : {}", result.score);
        println!("Gap Count    : {}", result.gap_indices.len());
        println!("Similarity   : {:.2}%", result.similarity_pct());
        println!("--------------------------------");
        println!("{}", result.pretty_print());
        println!("--------------------------------");
        
        println!("\n--- Semantic Diff ---");
        print_semantic_diff(item1, item2);
        println!("--------------------------------");
    } else {
        let mut diffs = Vec::new();
        get_semantic_diffs(item1, item2, &mut diffs);
        
        let output = serde_json::json!({
            "score": result.score,
            "similarity": result.similarity_pct(),
            "item_a": {
                "file": Path::new(save1_path).file_name().unwrap_or_default().to_string_lossy(),
                "index": index1,
                "code": item1.code.trim()
            },
            "item_b": {
                "file": Path::new(save2_path).file_name().unwrap_or_default().to_string_lossy(),
                "index": index2,
                "code": item2.code.trim()
            },
            "semantic_diffs": diffs
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    }

    Ok(())
}

fn get_semantic_diffs(a: &Item, b: &Item, diffs: &mut Vec<String>) {
    if a.header.version != b.header.version {
        diffs.push(format!("[HEADER] Version: {} -> {}", a.header.version, b.header.version));
    }
    if a.code != b.code {
        diffs.push(format!("[CODE] {} -> {}", a.code.trim(), b.code.trim()));
    }
    if a.header.id != b.header.id {
        diffs.push(format!("[ID] {:?} -> {:?}", a.header.id, b.header.id));
    }
    if a.header.level != b.header.level {
        diffs.push(format!("[LEVEL] {:?} -> {:?}", a.header.level, b.header.level));
    }
    if a.header.quality != b.header.quality {
        diffs.push(format!("[QUALITY] {:?} -> {:?}", a.header.quality, b.header.quality));
    }
    if a.header.flags != b.header.flags {
        diffs.push(format!("[FLAGS] 0x{:08X} -> 0x{:08X}", a.header.flags, b.header.flags));
    }
    
    if a.defense != b.defense {
        diffs.push(format!("[DEFENSE] {:?} -> {:?}", a.defense, b.defense));
    }
    if a.max_durability != b.max_durability {
        diffs.push(format!("[MAX DUR] {:?} -> {:?}", a.max_durability, b.max_durability));
    }
    if a.current_durability != b.current_durability {
        diffs.push(format!("[CUR DUR] {:?} -> {:?}", a.current_durability, b.current_durability));
    }
    if a.quantity != b.quantity {
        diffs.push(format!("[QUANTITY] {:?} -> {:?}", a.quantity, b.quantity));
    }
    
    append_property_diffs("Properties", &a.properties, &b.properties, diffs);
    append_property_diffs("Runeword Attributes", &a.runeword_attributes, &b.runeword_attributes, diffs);
}

fn append_property_diffs(label: &str, a: &[ItemProperty], b: &[ItemProperty], diffs: &mut Vec<String>) {
    if a == b {
        return;
    }
    let max_props = a.len().max(b.len());
    for i in 0..max_props {
        let p1 = a.get(i);
        let p2 = b.get(i);
        match (p1, p2) {
            (Some(p1), Some(p2)) => {
                if p1 != p2 {
                    if p1.stat_id != p2.stat_id {
                        diffs.push(format!("[{}] {}: StatID {} ({}) -> StatID {} ({})", label, i, p1.stat_id, p1.name, p2.stat_id, p2.name));
                    } else {
                        diffs.push(format!("[{}] {}: {} (ID {}) Value {} -> {}", label, i, p1.name, p1.stat_id, p1.value, p2.value));
                    }
                }
            }
            (Some(p1), None) => {
                diffs.push(format!("[{}] {}: {} (ID {}) REMOVED", label, i, p1.name, p1.stat_id));
            }
            (None, Some(p2)) => {
                diffs.push(format!("[{}] {}: {} (ID {}) NEW", label, i, p2.name, p2.stat_id));
            }
            (None, None) => unreachable!(),
        }
    }
}

fn print_semantic_diff(a: &Item, b: &Item) {
    let mut diffs = Vec::new();
    get_semantic_diffs(a, b, &mut diffs);
    for diff in diffs {
        println!("  {}", diff);
    }
}

fn load_items(bytes: &[u8], huffman: &HuffmanTree, path: &str) -> Vec<Item> {
    let version = if bytes.len() >= 8 {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    let alpha_mode = version == 105;

    match Item::read_player_items(bytes, huffman, alpha_mode) {
        Ok(items) => items,
        Err(e) => {
            eprintln!("Warning: Error reading items from {}: {}", path, e);
            Vec::new()
        }
    }
}
