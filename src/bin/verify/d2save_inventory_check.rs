use d2r_core::inventory::InventoryGrid;
use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::process;

fn main() {
    let mut parser = ArgParser::new("d2save_inventory_check").description(
        "Checks inventory integrity, collisions, and out-of-bounds items in a D2R save file",
    );

    parser.add_spec(ArgSpec::positional(
        "save_file",
        "path to the save file (.d2s)",
    ));

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let path = parsed.get("save_file").unwrap();
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[ERROR] Cannot read '{}': {}", path, e);
            process::exit(1);
        }
    };

    println!("=== Inventory Integrity Check: {} ===", path);

    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let huffman = HuffmanTree::new();
    let items = match Item::read_player_items(&bytes, &huffman, version == 105) {
        Ok(items) => items,
        Err(e) => {
            eprintln!("[ERROR] Failed to read player items: {}", e);
            process::exit(1);
        }
    };

    let items: Vec<_> = items
        .into_iter()
        .filter(|item| {
            let trimmed_code = item.code.trim();
            !item.is_residue()
                && !trimmed_code.is_empty()
                && trimmed_code != "ks d"
                && trimmed_code != "b7ts"
                && trimmed_code != "wyws"
        })
        .collect();

    println!("  Analyzing {} items in Player section...", items.len());

    for (i, item) in items.iter().enumerate() {
        let category = d2r_core::inventory::get_item_category(&item.code);
        let route = inventory_validation_route(item, version == 105);
        println!(
            "  - Item[{:>2}]: code='{}' -> category='{}' -> route='{}'",
            i, item.code, category, route
        );
    }
    println!();

    let inventory_candidates: Vec<_> = items
        .into_iter()
        .filter(|item| inventory_validation_route(item, version == 105) == "inventory")
        .collect();

    println!(
        "  Validating {} inventory candidates after routing...",
        inventory_candidates.len()
    );

    let errors = InventoryGrid::validate_logical_integrity(&inventory_candidates, 10, 4);

    if errors.is_empty() {
        println!("\x1b[32m[OK] No inventory collisions or out-of-bounds detected.\x1b[0m");
    } else {
        println!(
            "\x1b[31m[FAILED] Found {} inventory errors:\x1b[0m",
            errors.len()
        );
        for (i, err) in errors.iter().enumerate() {
            println!("  {:>2}. {}", i + 1, err);
        }
    }

    println!("\n[Final Inventory Layout]");
    let grid = InventoryGrid::from_save_bytes(&bytes, &huffman);
    grid.debug_print();
}

fn inventory_validation_route(item: &Item, is_alpha: bool) -> &'static str {
    let is_true_pot = is_true_potion(&item.code);

    if item.mode == 1 {
        if is_true_pot {
            "belt"
        } else {
            "equipment"
        }
    } else if item.mode == 2 {
        if is_alpha && !is_true_pot && item.location != 2 {
            match item.location {
                4 => "stash",
                7 => "cube",
                _ => "inventory",
            }
        } else {
            "belt"
        }
    } else {
        match item.location {
            0 => {
                if is_true_pot && item.x == 0 && item.y == 0 {
                    "belt"
                } else {
                    "inventory"
                }
            }
            2 => "belt",
            4 => "stash",
            7 => "cube",
            _ => "inventory",
        }
    }
}

fn is_true_potion(code: &str) -> bool {
    let trimmed = code.trim().to_lowercase();
    trimmed.starts_with("hp") || trimmed.starts_with("mp") || trimmed.starts_with("rv")
}
