use std::fs;
use anyhow::{bail, Context};
use d2r_core::save::{map_core_sections, rebuild_status_and_player_items};
use d2r_core::item::{Item, HuffmanTree, ItemEditorExt};
use d2r_core::verify::args::{ArgParser, ArgSpec};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_mutate");
    parser.add_arg("input").description("Input save file (.d2s)");
    parser.add_opt("output").short('o').long("output").description("Output save file (.d2s)").required();
    
    // Legacy marker mutations
    parser.add_opt("shift-marker").long("shift-marker").description("Shift marker <NAME> <OFFSET>").value_count(2);
    parser.add_opt("delete-marker").long("delete-marker").description("Delete marker <NAME>").value_count(1);
    
    // New item mutations
    parser.add_opt("item-index").long("item-index").description("0-based index of the item to mutate");
    parser.add_opt("stat").long("stat").description("Stat ID to mutate");
    parser.add_opt("value").long("value").description("New value for the stat");
    parser.add_opt("defense").long("defense").description("Set defense value");
    parser.add_flag("force-fix").long("force-fix").description("Force checksum and size finalization (required for v105 logic updates)");

    let parsed = match parser.parse(std::env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(d2r_core::verify::args::ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(d2r_core::verify::args::ArgError::Error(e)) => {
            bail!("error: {}\n\n{}", e, parser.usage());
        }
    };

    let input_path = parsed.get("input").unwrap();
    let output_path = parsed.get("output").unwrap();

    let mut bytes = fs::read(input_path).context("Failed to read input file")?;
    
    // Validate version 105
    if bytes.len() < 8 {
        bail!("Input file is too small");
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != 105 {
        bail!("Only Alpha v105 is supported. Found version {}", version);
    }

    let map = map_core_sections(&bytes).context("Failed to map core sections")?;
    
    let huffman = HuffmanTree::new();
    let is_alpha = version == 105;
    let force_fix = parsed.is_set("force-fix");

    if let Some(shift_args) = parsed.get_vec("shift-marker") {
        let name = &shift_args[0];
        let offset: isize = shift_args[1].parse().context("Invalid shift offset")?;
        mutate_marker(&mut bytes, &map, name, Some(offset))?;
    } else if let Some(delete_args) = parsed.get_vec("delete-marker") {
        let name = &delete_args[0];
        mutate_marker(&mut bytes, &map, name, None)?;
    } else if let Some(item_idx_str) = parsed.get("item-index") {
        let idx: usize = item_idx_str.parse().context("Invalid item index")?;
        let mut items = Item::read_player_items(&bytes, &huffman, is_alpha).context("Failed to read items")?;
        
        if idx >= items.len() {
            bail!("Item index {} out of bounds (found {} items)", idx, items.len());
        }
        
        {
            // Slice 2: Prevent mutating Opaque items
            if items[idx].modules.iter().any(|m| matches!(m, d2r_core::item::ItemModule::Opaque(_))) {
                bail!("Cannot mutate an Opaque (unparsable) item at index {}.", idx);
            }

            let mut editor = items[idx].edit();
            let mut modified = false;

            if let Some(def_str) = parsed.get("defense") {
                let def: u32 = def_str.parse().context("Invalid defense value")?;
                editor.set_defense(def);
                modified = true;
            }

            if let (Some(stat_str), Some(val_str)) = (parsed.get("stat"), parsed.get("value")) {
                let stat_id: u32 = stat_str.parse().context("Invalid stat ID")?;
                let val: i32 = val_str.parse().context("Invalid stat value")?;
                editor.set_stat(stat_id, val);
                modified = true;
            }

            if !modified {
                bail!("Item index provided but no mutation operation specified (--stat/--value or --defense).");
            }
            editor.commit();
        }

        println!("Mutating item at index {} (code: {})", idx, items[idx].body.code);

        let mut rebuilt = rebuild_status_and_player_items(
            &bytes, None, None, None, None, None, &items, &huffman
        ).context("Failed to rebuild save with mutated items")?;
        
        d2r_core::save::finalize_save_bytes(&mut rebuilt, force_fix).context("Failed to finalize save bytes")?;
        bytes = rebuilt;
        println!("Successfully rebuilt save with mutated item (force_fix={}).", force_fix);
    } else {
        bail!("No mutation operation specified. Use --shift-marker, --delete-marker, or --item-index.");
    }

    fs::write(output_path, &bytes).context("Failed to write output file")?;
    println!("Successfully mutated save and saved to {}", output_path);

    Ok(())
}

fn mutate_marker(bytes: &mut [u8], map: &d2r_core::save::SaveSectionMap, name: &str, shift: Option<isize>) -> anyhow::Result<()> {
    let (pos, marker_bytes) = match name {
        "Woo!" => (map.woo_pos, b"Woo!".as_slice()),
        "WS" => (map.ws_pos, b"WS".as_slice()),
        "w4" => (map.w4_pos, b"w4".as_slice()),
        _ => bail!("Unknown marker name: {}. Supported: Woo!, WS, w4", name),
    };

    let original_pos = pos.ok_or_else(|| anyhow::anyhow!("Marker {} not found in input file", name))?;
    let len = marker_bytes.len();

    println!("Original marker {} at range 0x{:X}..0x{:X}", name, original_pos, original_pos + len);

    // Zero out original
    for i in 0..len {
        bytes[original_pos + i] = 0;
    }

    if let Some(s) = shift {
        let new_pos_i = original_pos as isize + s;
        if new_pos_i < 0 {
            bail!("Shifted marker {} out of bounds (negative offset)", name);
        }
        let new_pos = new_pos_i as usize;
        if new_pos + len > bytes.len() {
            bail!("Shifted marker {} out of bounds (beyond EOF)", name);
        }
        
        for i in 0..len {
            bytes[new_pos + i] = marker_bytes[i];
        }
        println!("Shifted marker {} to range 0x{:X}..0x{:X}", name, new_pos, new_pos + len);
    } else {
        println!("Deleted marker {} (zero-filled)", name);
    }

    println!("Note: Checksum was NOT recalculated.");
    Ok(())
}