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
    parser.add_flag("alpha-rhythm-grid", "display 72/80-bit rhythmic grid for Alpha v105").long("alpha-rhythm-grid");
    parser.add_flag("verbose-markers", "display internal scanner confidence scores and rejected markers").long("verbose-markers");

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
    let alpha_grid = parsed.is_set("alpha-rhythm-grid");
    let verbose_markers = parsed.is_set("verbose-markers");
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

    use d2r_core::save::map_core_sections;
    if let Ok(map) = map_core_sections(&bytes) {
        println!();
        println!("[CORE SECTIONS]");
        println!("  gf_pos:  Offset {:>5} (bit {:>6}) | Attributes", map.gf_pos, map.gf_pos * 8);
        println!("  if_pos:  Offset {:>5} (bit {:>6}) | Skills", map.if_pos, map.if_pos * 8);
        let gf_len = map.if_pos.saturating_sub(map.gf_pos);
        println!("  (gf delta: {} bytes / {} bits)", gf_len, gf_len * 8);

        if save.header.version == 105 {
            println!();
            println!("[FORENSIC MARKERS (Alpha v105)]");
            if let Some(pos) = map.woo_pos { println!("  [Woo!] Offset {pos:>5} (bit {:>6}) | Progression (Quests)", pos * 8); }
            if let Some(pos) = map.ws_pos { println!("  [WS  ] Offset {pos:>5} (bit {:>6}) | Progression (Waypoints)", pos * 8); }
            if let Some(pos) = map.w4_pos { println!("  [w4  ] Offset {pos:>5} (bit {:>6}) | NPC Data", pos * 8); }
            if let Some(pos) = map.jf_pos { println!("  [jf  ] Offset {pos:>5} (bit {:>6}) | Mercenary Marker", pos * 8); }
            if let Some(pos) = map.kf_pos { println!("  [kf  ] Offset {pos:>5} (bit {:>6}) | Mercenary Footer Start", pos * 8); }
            if let Some(pos) = map.lf_pos { println!("  [lf  ] Offset {pos:>5} (bit {:>6}) | Mercenary Footer End", pos * 8); }
        }
    }

    println!();
    println!("[JM MARKERS]");

    let jm_positions = find_jm_markers(&bytes);

    let section_labels = [
        "Player Items",
        "Corpse Items",
        "Mercenary Items",
        "Iron Golem",
    ];

    let mut last_jm_pos = 0;
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
        if idx > 0 {
            let delta = pos - last_jm_pos;
            println!("    (Delta from prev JM: {} bytes / {} bits)", delta, delta * 8);
        }
        last_jm_pos = pos;

        if item_count > 0 || section_size > 6 {
            use d2r_core::item::{Item, HuffmanTree};
            let huffman = HuffmanTree::new();
            let is_alpha = save.header.version == 105;
            let section_data = &bytes[pos..next_pos];
            match Item::read_section(section_data, pos as u64 * 8, item_count, &huffman, is_alpha, verbose_markers) {
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
                        for module in &item.modules {
                            match module {
                                d2r_core::item::ItemModule::SemiOpaque { reason, .. } => {
                                    println!("        [SemiOpaque] {} bits | Reason: {}", item.total_bits, reason);
                                }
                                d2r_core::item::ItemModule::Residue(_) => {
                                    println!("        [Residue] {} bits", item.total_bits);
                                }
                                _ => {}
                            }
                        }
                    }

                    if alpha_grid || verbose_markers {
                        render_heatmap(section_data, &items, pos as u64 * 8, is_alpha, alpha_grid, verbose_markers);
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

    println!();
    println!("[SUMMARY]");
    println!("  Total JM sections: {}", jm_positions.len());
    if let Some(&first_jm) = jm_positions.first() {
        println!("  Header + pre-item data: {} bytes", first_jm);
    }
    
    Ok(())
}

fn render_heatmap(
    data: &[u8], 
    items: &[d2r_core::item::Item], 
    section_bit_offset: u64, 
    alpha: bool, 
    show_grid: bool, 
    show_markers: bool
) {
    use d2r_core::item::ItemModule;
    use colored::Colorize;

    println!("\n    [BIT HEATMAP]");
    
    // 1. Display expected vs parsed count
    let expected_count = if data.len() >= 4 { u16::from_le_bytes([data[2], data[3]]) } else { 0 };
    let parsed_count = items.iter().filter(|it| !it.is_residue()).count();
    println!("      JM Expected: {} | Parsed: {} | Delta: {}", 
        expected_count, 
        parsed_count, 
        (parsed_count as i32 - expected_count as i32)
    );

    // 2. Display Forensic Markers if requested
    if show_markers && alpha {
        println!("      Forensic Markers (Scanner Scores):");
        // Re-run scanner in verbose mode to get all markers
        use d2r_core::domain::item::scanner::{self, MarkerStatus};
        let huffman = d2r_core::item::HuffmanTree::new();
        let markers = scanner::scan_item_markers(data, &huffman, true, section_bit_offset, Some(expected_count), true);
        for marker in markers {
            let status_str = format!("{:?}", marker.status);
            let status_colored = match marker.status {
                MarkerStatus::Accepted => status_str.green(),
                MarkerStatus::Rejected => status_str.yellow(),
                MarkerStatus::Phantom => status_str.red(),
            };
            println!("        - Bit {:5}: [{:<4}] Score: {:4} | Status: {}", 
                marker.offset, marker.code, marker.score, status_colored
            );
        }
    }

    // 3. Render Bitstream with Rhythmic Grid
    if show_grid && alpha {
        println!("      Bitstream (80-bit periodic grid):");
        let total_bits = (data.len() * 8) as u64;
        let mut bit_pos = 0;
        
        while bit_pos < total_bits {
            if bit_pos % 80 == 0 {
                print!("{}", "|".bright_black());
            } else if bit_pos % 8 == 0 {
                print!("{}", ".".bright_black());
            }

            // Find item covering this bit
            let item = items.iter().find(|it| {
                let rel_start = it.range.start - section_bit_offset;
                let rel_end = it.range.end - section_bit_offset;
                bit_pos >= rel_start && bit_pos < rel_end
            });

            let bit_val = if (data[(bit_pos / 8) as usize] & (1 << (bit_pos % 8))) != 0 { "1" } else { "0" };
            
            if let Some(it) = item {
                if it.is_residue() {
                    print!("{}", bit_val.truecolor(80, 80, 80)); // Dark Gray for residue
                } else if it.modules.iter().any(|m| matches!(m, ItemModule::SemiOpaque { .. })) {
                    print!("{}", bit_val.yellow()); // Yellow for SemiOpaque
                } else if it.is_opaque() {
                    print!("{}", bit_val.red()); // Red for Opaque
                } else {
                    print!("{}", bit_val.green()); // Green for normal
                }
            } else {
                print!("{}", bit_val.bright_black());
            }

            bit_pos += 1;
            if bit_pos % 80 == 0 {
                println!(" (Bit {})", bit_pos);
            }
        }
        println!();
    }
}
