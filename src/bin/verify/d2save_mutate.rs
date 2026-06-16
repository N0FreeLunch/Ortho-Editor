use anyhow::{Context, bail};
use d2r_core::item::{HuffmanTree, Item, ItemEditorExt};
use d2r_core::save::{map_core_sections, rebuild_status_and_player_items};
use d2r_core::verify::args::ArgParser;
use std::fs;

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_mutate");
    parser.add_arg("input", "Input save file (.d2s)");
    parser
        .add_opt("output", "Output save file (.d2s)")
        .short('o')
        .long("output")
        .required();

    // Legacy marker mutations
    parser
        .add_opt("shift-marker", "Shift marker <NAME> <OFFSET>")
        .long("shift-marker")
        .value_count(2);
    parser
        .add_opt("delete-marker", "Delete marker <NAME>")
        .long("delete-marker")
        .value_count(1);

    // New item mutations
    parser
        .add_opt("item-index", "0-based index of the item to mutate")
        .long("item-index");
    parser.add_opt("stat", "Stat ID to mutate").long("stat");
    parser
        .add_opt("value", "New value for the stat")
        .long("value");
    parser
        .add_opt("defense", "Set defense value")
        .long("defense");
    parser
        .add_flag(
            "force-fix",
            "Force checksum and size finalization (required for v105 logic updates)",
        )
        .long("force-fix");

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
        mutate_marker_and_finalize(&mut bytes, &map, name, Some(offset), force_fix)?;
    } else if let Some(delete_args) = parsed.get_vec("delete-marker") {
        let name = &delete_args[0];
        mutate_marker_and_finalize(&mut bytes, &map, name, None, force_fix)?;
    } else if let Some(item_idx_str) = parsed.get("item-index") {
        let idx: usize = item_idx_str.parse().context("Invalid item index")?;
        let mut items =
            Item::read_player_items(&bytes, &huffman, is_alpha).context("Failed to read items")?;

        if idx >= items.len() {
            bail!(
                "Item index {} out of bounds (found {} items)",
                idx,
                items.len()
            );
        }

        {
            // Slice 1: Treat forensic isolation cases as read-only mutation targets.
            if is_non_editable_forensic_item(&items[idx]) {
                bail!(
                    "Cannot mutate a non-editable forensic item (Opaque/SemiOpaque/Residue) at index {}.",
                    idx
                );
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
                bail!(
                    "Item index provided but no mutation operation specified (--stat/--value or --defense)."
                );
            }
            editor.commit();
        }

        println!(
            "Mutating item at index {} (code: {})",
            idx, items[idx].body.code
        );

        let mut rebuilt =
            rebuild_status_and_player_items(&bytes, None, None, None, None, None, &items, &huffman)
                .context("Failed to rebuild save with mutated items")?;

        d2r_core::save::finalize_save_bytes(&mut rebuilt, force_fix)
            .context("Failed to finalize save bytes")?;
        bytes = rebuilt;
        println!(
            "Successfully rebuilt save with mutated item (force_fix={}).",
            force_fix
        );
    } else {
        bail!(
            "No mutation operation specified. Use --shift-marker, --delete-marker, or --item-index."
        );
    }

    fs::write(output_path, &bytes).context("Failed to write output file")?;
    println!("Successfully mutated save and saved to {}", output_path);

    Ok(())
}

fn mutate_marker(
    bytes: &mut [u8],
    map: &d2r_core::save::SaveSectionMap,
    name: &str,
    shift: Option<isize>,
) -> anyhow::Result<()> {
    let (pos, marker_bytes) = match name {
        "Woo!" => (map.woo_pos, b"Woo!".as_slice()),
        "WS" => (map.ws_pos, b"WS".as_slice()),
        "w4" => (map.w4_pos, b"w4".as_slice()),
        _ => bail!("Unknown marker name: {}. Supported: Woo!, WS, w4", name),
    };

    let original_pos =
        pos.ok_or_else(|| anyhow::anyhow!("Marker {} not found in input file", name))?;
    let len = marker_bytes.len();

    println!(
        "Original marker {} at range 0x{:X}..0x{:X}",
        name,
        original_pos,
        original_pos + len
    );

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
        println!(
            "Shifted marker {} to range 0x{:X}..0x{:X}",
            name,
            new_pos,
            new_pos + len
        );
    } else {
        println!("Deleted marker {} (zero-filled)", name);
    }

    Ok(())
}

fn mutate_marker_and_finalize(
    bytes: &mut Vec<u8>,
    map: &d2r_core::save::SaveSectionMap,
    name: &str,
    shift: Option<isize>,
    force_fix: bool,
) -> anyhow::Result<()> {
    mutate_marker(bytes, map, name, shift)?;
    d2r_core::save::finalize_save_bytes(bytes, force_fix).context("Failed to finalize save bytes")?;
    Ok(())
}

fn is_non_editable_forensic_item(item: &Item) -> bool {
    item.is_opaque() || item.is_semi_opaque() || item.is_residue()
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_core::save::{recalculate_checksum, Save};
    use std::path::PathBuf;

    fn editable_item() -> Item {
        let mut item = Item::default();
        item.code = "cap".to_string();
        item.body.code = "cap".to_string();
        item
    }

    fn fixture_bytes(name: &str) -> Vec<u8> {
        let repo_root = env!("CARGO_MANIFEST_DIR");
        let path = PathBuf::from(repo_root)
            .join("tests")
            .join("fixtures")
            .join("savegames")
            .join("original")
            .join(name);
        fs::read(path).expect("fixture should exist")
    }

    #[test]
    fn opaque_item_is_non_editable() {
        let mut item = editable_item();
        item.modules
            .push(d2r_core::item::ItemModule::Opaque(vec![true, false]));

        assert!(is_non_editable_forensic_item(&item));
    }

    #[test]
    fn semi_opaque_item_is_non_editable() {
        let mut item = editable_item();
        item.modules.push(d2r_core::item::ItemModule::SemiOpaque {
            body_bits: vec![true, false, true],
            reason: "forensic isolation".to_string(),
        });

        assert!(is_non_editable_forensic_item(&item));
    }

    #[test]
    fn residue_item_is_non_editable() {
        assert!(is_non_editable_forensic_item(&Item::default()));
    }

    #[test]
    fn normal_item_remains_editable() {
        assert!(!is_non_editable_forensic_item(&editable_item()));
    }

    #[test]
    fn marker_shift_is_already_finalized() -> anyhow::Result<()> {
        let mut bytes = fixture_bytes("amazon_v105_act2_start.d2s");
        let map = map_core_sections(&bytes).context("Failed to map core sections")?;

        mutate_marker_and_finalize(&mut bytes, &map, "Woo!", Some(1), false)?;

        let save = Save::from_bytes(&bytes).context("Failed to parse finalized bytes")?;
        assert_eq!(save.header.file_size as usize, bytes.len());

        let recalculated = recalculate_checksum(&bytes)?;
        assert_eq!(save.header.checksum, recalculated);

        Ok(())
    }
}
