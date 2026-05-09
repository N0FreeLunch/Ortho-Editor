use std::env;
use std::fs;
use d2r_core::verify::args::{ArgParser, ArgError};

use d2r_core::save::{
    ACTIVE_WEAPON_OFFSET, CHAR_CLASS_OFFSET, CHAR_LEVEL_OFFSET, CHAR_NAME_OFFSET,
    LAST_PLAYED_OFFSET, Save, class_name, find_jm_markers,
};

fn main() -> anyhow::Result<()> {
    let mut parser = ArgParser::new("d2save_map")
        .description("Maps and summarizes the major sections and JM markers of a D2R save file");

    parser.add_arg("save_file", "path to the save file (.d2s)");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            return Ok(());
        }
        Err(ArgError::Error(e)) => {
            anyhow::bail!("{}\n\n{}", e, parser.usage());
        }
    };

    let path = parsed.get("save_file").unwrap();
    let bytes = fs::read(path).map_err(|e| anyhow::anyhow!("Cannot read '{}': {}", path, e))?;

    println!("=== Section Map: {} ({} bytes) ===", path, bytes.len());

    let save = Save::from_bytes(&bytes).map_err(|err| anyhow::anyhow!("Cannot parse D2R header: {}", err))?;

    println!();
    println!("[HEADER]");
    println!("  Offset  0 | Magic:         0x{:08X}", save.header.magic);
    println!("  Offset  4 | Version:       {}", save.header.version);
    println!(
        "  Offset  8 | File Size:     {} bytes",
        save.header.file_size
    );
    println!(
        "  Offset 12 | Checksum:      0x{:08X}",
        save.header.checksum
    );
    println!(
        "  Offset {ACTIVE_WEAPON_OFFSET:<2} | Active Weapon: {}",
        save.header.active_weapon
    );
    println!(
        "  Offset {CHAR_CLASS_OFFSET:<2} | Char Class:    {} ({})",
        class_name(save.header.char_class),
        save.header.char_class
    );
    println!(
        "  Offset {CHAR_LEVEL_OFFSET:<2} | Char Level:    {}",
        save.header.char_level
    );
    println!(
        "  Offset {LAST_PLAYED_OFFSET} | Last Played:   0x{:08X}",
        save.header.last_played
    );
    println!(
        "  Offset {CHAR_NAME_OFFSET} | Char Name:     '{}'",
        save.header.char_name
    );

    println!();
    println!("[JM MARKERS]");

    let jm_positions = find_jm_markers(&bytes);

    let section_labels = [
        "Player Items",
        "Corpse Items",
        "Mercenary Items",
        "Iron Golem",
    ];

    for (idx, &pos) in jm_positions.iter().enumerate() {
        let label = section_labels
            .get(idx)
            .copied()
            .unwrap_or("Unknown Section");
        let item_count = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
        let next_pos = jm_positions.get(idx + 1).copied().unwrap_or(bytes.len());
        let section_size = next_pos - pos;

        println!(
            "  [JM #{idx}] Offset {pos:>5} (bit {:>6}) | {label:<20} | count={item_count}, section_bytes={section_size}",
            pos * 8
        );

        if item_count > 0 {
            use d2r_core::item::{Item, HuffmanTree};
            let huffman = HuffmanTree::new();
            let is_alpha = save.header.version == 105;
            let section_data = &bytes[pos + 4..next_pos];
            match Item::read_section(section_data, (pos as u64 + 4) * 8, item_count, &huffman, is_alpha) {
                Ok(items) => {
                    println!("    Parsed {} items:", items.len());
                    for (item_idx, item) in items.iter().enumerate() {
                        let quality_str = match item.header.quality {
                            Some(q) => format!("{:?}", q),
                            None => "None".to_string(),
                        };
                        println!("      [{}] {} (v={}, quality={}, mode={}, loc={}) at bit {}", 
                            item_idx, item.code.trim(), item.header.version, quality_str, item.mode, item.location, item.range.start);
                        if item.code == "Opaque" {
                            println!("        [Opaque] {} bits", item.total_bits);
                        }
                        for prop in &item.properties {
                            println!("        Prop: ID={} (raw={})", prop.stat_id, prop.raw_value);
                        }
                        if !item.socketed_items.is_empty() {
                            println!("        Socketed Items ({}):", item.socketed_items.len());
                            for child in &item.socketed_items {
                                println!("          Child: {}", child.code.trim());
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("    [ERROR] Failed to parse items: {}", e);
                }
            }
        }
    }

    if jm_positions.is_empty() {
        println!("  [WARN] No JM markers found in file");
    }

    if save.header.version == 105 {
        use d2r_core::save::map_core_sections;
        if let Ok(map) = map_core_sections(&bytes) {
            println!();
            println!("[FORENSIC MARKERS (Alpha v105)]");
            if let Some(pos) = map.woo_pos { println!("  [Woo!] Offset {pos:>5} (bit {:>6}) | Progression (Quests)", pos * 8); }
            if let Some(pos) = map.ws_pos { println!("  [WS  ] Offset {pos:>5} (bit {:>6}) | Progression (Waypoints)", pos * 8); }
            if let Some(pos) = map.w4_pos { 
                println!("  [w4  ] Offset {pos:>5} (bit {:>6}) | NPC Data (Section Start)", pos * 8);
            }
            if let Some(pos) = map.jf_pos { println!("  [jf  ] Offset {pos:>5} (bit {:>6}) | Mercenary Marker", pos * 8); }

            // Slice 10: Hybrid Mercenary Decoding (Axiom 0328)
            use d2r_core::domain::forensic::v105::MercenaryState;
            let w4_data_option = map.w4_pos.map(|pos| {
                let w4_end = map.jf_pos.unwrap_or(bytes.len().min(pos + 40));
                bytes.get(pos + 2..w4_end).unwrap_or(&[])
            });
            let merc = MercenaryState::from_hybrid(&bytes, w4_data_option);
            
            println!();
            println!("[MERCENARY (Hybrid Decoded)]");
            println!("  Hireling ID: {} (XP: {})", merc.hireling_id, merc.experience);
            if merc.name_id > 0 {
                println!("  Name ID:     {}", merc.name_id);
            }

            println!();
            println!("[HEADER (Mercenary Raw Anchors)]");
            println!("  Offset 169 | Hireling ID:   {}", bytes.get(169).copied().unwrap_or(0));
            println!("  Offset 171 | Experience:    {} (0x{:08X})", 
                u32::from_le_bytes(bytes.get(171..175).and_then(|b| b.try_into().ok()).unwrap_or([0; 4])),
                u32::from_le_bytes(bytes.get(171..175).and_then(|b| b.try_into().ok()).unwrap_or([0; 4]))
            );
            println!("  Offset 175 | Level (?):     Not found (plain byte search failed)");
            println!("  Offset 176..185: {:02X?}", bytes.get(176..185).unwrap_or(&[]));

            if let (Some(kf), Some(lf)) = (map.kf_pos, map.lf_pos) {
                use d2r_core::domain::forensic::v105::MercenaryFooter;
                let footer_bytes = bytes.get(kf..lf + 2).unwrap_or(&[]);
                let footer = MercenaryFooter::from_bytes(footer_bytes);
                
                println!("  [kf  ] Offset {kf:>5} (bit {:>6}) | Mercenary Footer Start", kf * 8);
                println!("  [lf  ] Offset {lf:>5} (bit {:>6}) | Mercenary Footer End", lf * 8);
                println!("  [MERC] Footer Payload: {:02X?} (Standard: {})", footer.raw, footer.is_standard());
            } else {
                if let Some(pos) = map.kf_pos { println!("  [kf  ] Offset {pos:>5} (bit {:>6}) | Mercenary Footer Start", pos * 8); }
                if let Some(pos) = map.lf_pos { println!("  [lf  ] Offset {pos:>5} (bit {:>6}) | Mercenary Footer End", pos * 8); }
            }
        }
    }

    println!();
    println!("[SUMMARY]");
    println!("  Total JM sections: {}", jm_positions.len());
    if let Some(&first_jm) = jm_positions.first() {
        println!("  Header + pre-item data: {} bytes", first_jm);
    }
    if let (Some(&first_jm), Some(&second_jm)) = (jm_positions.get(0), jm_positions.get(1)) {
        let player_section_bytes = second_jm - first_jm;
        println!("  Player item section:    {} bytes", player_section_bytes);
    }

    Ok(())
}
