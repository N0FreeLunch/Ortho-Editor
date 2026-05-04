use d2r_core::item::{HuffmanTree, Item, ItemBitRange, ItemProperty, ItemQuality};
use d2r_core::save::{
    AttributeEntry, AttributeSection, Save, finalize_save_bytes, map_core_sections,
};
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("DEBUG Args: {:?}", args);
    if args.len() < 6 {
        println!("Status: Fuzzing Tool Initialized");
        println!(
            "Usage: cargo run --example luck_fuzzer -- <input.d2s> <output.d2s> <mode:gf|if|af> <id> <value> [bits]"
        );
        process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];
    let mode = &args[3];
    let stat_id: u32 = args[4].parse().expect("Invalid stat_id");
    let value: u32 = args[5].parse().expect("Invalid value");

    let mut item_index: usize = 10;
    let mut gf_bits: u32 = 10;

    let trailing_args = &args[6..];
    match mode.as_str() {
        "gf" => {
            if let Some(arg) = trailing_args.get(0) {
                gf_bits = arg.parse().expect("Invalid bits for gf mode");
            }
        }
        "if" | "af" => {
            let mut i = 0;
            while i < trailing_args.len() {
                match trailing_args[i].as_str() {
                    "--item-index" => {
                        if i + 1 < trailing_args.len() {
                            item_index = trailing_args[i + 1].parse().expect("Invalid item index");
                            i += 2;
                        } else {
                            panic!("Missing value for --item-index");
                        }
                    }
                    _ => {
                        panic!(
                            "Unknown trailing argument for {} mode: {}",
                            mode, trailing_args[i]
                        );
                    }
                }
            }
        }
        _ => {}
    }

    println!("Mode      : {}", mode);
    println!("Target    : ID {} (Value {})", stat_id, value);
    if mode == "if" || mode == "af" {
        println!("Item Index: {}", item_index);
    } else if mode == "gf" {
        println!("GF Bits   : {}", gf_bits);
    }

    let bytes = fs::read(input_path).expect("Failed to read input file");
    let huffman = HuffmanTree::new();

    // Parse core sections
    let map = map_core_sections(&bytes).expect("Failed to map sections");
    let mut attrs = AttributeSection::parse(&bytes, map.gf_pos, map.if_pos).expect("Failed to parse attributes");
    let mut items = Item::read_player_items(&bytes, &huffman, true).unwrap_or_default();

    match mode.as_str() {
        "gf" => {
            if stat_id == 511 {
                println!("Error: stat_id 511 is the 'gf' terminator and cannot be fuzzed.");
                process::exit(1);
            }
            println!("Mutating Character Stats (gf)...");
            attrs.entries.retain(|e| e.stat_id != stat_id);
            let mut val_bits = Vec::new();
            for i in 0..gf_bits {
                val_bits.push((value >> i) & 1 != 0);
            }
            attrs.entries.push(AttributeEntry {
                stat_id,
                param: 0,
                raw_value: 0,
                opaque_bits: Some(val_bits),
            });
        }
        "if" => {
            println!("Mutating Item Properties (if) at index {}...", item_index);
            if let Some(item) = items.get_mut(item_index) {
                println!(
                    "Target Item: {} [Old Quality: {:?}, Version: {}]",
                    item.code, item.quality, item.version
                );

                // Alpha item property value constraint
                if (item.version == 1 || item.version == 5) && value > 1 {
                    println!(
                        "Error: Value {} is > 1 for Alpha item (version {}). Bounded to 1-bit value.",
                        value, item.version
                    );
                    process::exit(1);
                }

                item.quality = Some(ItemQuality::Magic);
                item.is_compact = false;
                item.level = Some(10);
                item.flags &= !(1 << 21); // Clear compact
                item.flags |= 1 << 10; // Set magic bit

                item.properties.retain(|p| p.stat_id != stat_id);
                item.properties.push(ItemProperty {
                    stat_id,
                    name: format!("Fuzzed_{}", stat_id),
                    param: 0,
                    raw_value: value as i32,
                    value: value as i32,
                    range: ItemBitRange::default(),
                });
                item.properties_complete = true;
                item.bits.clear(); // FORCE RE-ENCODE
                println!(
                    "Set Item {} to Magic, Level 10, Stat ID {}",
                    item_index, stat_id
                );
            } else {
                println!("Error: Item {} not found.", item_index);
                process::exit(1);
            }
        }
        "af" => {
            println!("Mutating Item Affixes (af) at index {}...", item_index);
            if let Some(item) = items.get_mut(item_index) {
                println!(
                    "Target Item: {} [Old Quality: {:?}]",
                    item.code, item.quality
                );
                item.quality = Some(ItemQuality::Magic);
                item.is_compact = false;
                item.level = Some(10);
                item.flags &= !(1 << 21); // Clear compact
                item.flags |= 1 << 10; // Set magic bit
                item.magic_suffix = Some(stat_id as u16);
                item.bits.clear(); // FORCE RE-ENCODE
                println!(
                    "Set Item {} to Magic, Level 10, Magic Suffix ID {}",
                    item_index, stat_id
                );
            } else {
                println!("Error: Item {} not found.", item_index);
                process::exit(1);
            }
        }
        _ => {
            println!("Error: Unknown mode '{}'. Use gf, if, or af.", mode);
            process::exit(1);
        }
    }

    println!("Rebuilding save...");
    let mut save_bytes = d2r_core::save::rebuild_status_and_player_items(
        &bytes,
        Some(&attrs),
        None, // Skills
        None, // Quests
        None, // Waypoints
        None, // Expansion
        &items,
        &huffman,
    )
    .expect("Failed to rebuild save bytes");

    finalize_save_bytes(&mut save_bytes, true).expect("Failed to finalize checksums");
    fs::write(output_path, &save_bytes).expect("Failed to write output file");

    println!("--------------------------------------------------");
    println!("Outcome: gf/if Single-shot Injection Complete");
    println!("Input path : {}", input_path);
    println!("Output path: {}", output_path);
    println!("Mode       : {}", mode);
    println!("Stat ID    : {}", stat_id);
    println!("Value      : {}", value);
    if mode == "gf" {
        println!("Value Bits : {}", gf_bits);
        println!("Attrs Count: {}", attrs.entries.len());
    } else if mode == "if" || mode == "af" {
        println!("Item Index : {}", item_index);
    }
    println!("Result     : Success (Structural pass only)");
    println!("--------------------------------------------------");
}
