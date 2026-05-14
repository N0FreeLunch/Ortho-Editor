use d2r_core::save::{Save, map_core_sections};
use d2r_core::domain::forensic::v105::MercenaryState;
use std::fs;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!("Usage: cargo run --example mercenary_audit -- <file1> <file2> ...");
        return Ok(());
    }
    let files = args;

    for path in files {
        let bytes = fs::read(&path)?;
        let map = map_core_sections(&bytes).map_err(|e| anyhow::anyhow!("Map error: {}", e))?;
        
        let w4_data = map.w4_pos.map(|pos| {
            let w4_end = map.jf_pos.unwrap_or(bytes.len());
            &bytes[pos..w4_end]
        });

        println!("=== File: {} ===", path);
        if let Some(w4) = w4_data {
            let has_marker = w4.starts_with(b"w4");
            println!("w4 Section (pos={}): Marker Present: {}", map.w4_pos.unwrap_or(0), has_marker);
            println!("w4 Bytes (first 32): {:02X?}", &w4[..w4.len().min(32)]);
            
            // Re-decode using hybrid logic
            // Note: from_hybrid handles marker stripping internally (Axiom 0367)
            let merc = MercenaryState::from_hybrid(&bytes, Some(w4));

            let class_name = match merc.class_id {
                0 => if merc.hireling_id >= 8 { "Desert Warrior (Act 2)" } else { "Rogue (Act 1)" },
                1 => "Iron Wolf (Act 3)",
                9 => "Barbarian (Act 5)",
                _ => "Unknown",
            };

            let subtype_name = if merc.class_id == 1 { // Iron Wolf in v105 is Class 1
                match merc.subtype_id {
                    15 => "Fire",
                    16 => "Cold",
                    17 => "Lightning",
                    _ => "Unknown Element",
                }
            } else {
                "N/A"
            };

            println!("Hybrid Decoded:");
            println!("  Class:    {} ({})", merc.class_id, class_name);
            println!("  Subtype:  {} ({})", merc.subtype_id, subtype_name);
            println!("  ID (H169):{}", merc.hireling_id);
            println!("  Experience: {} (0x{:08X})", merc.experience, merc.experience);
            println!("  Name ID:   {}", merc.name_id);
            
            // Axiom 0367 Alignment Check
            let c_off = if has_marker { 6 } else { 4 };
            let raw_class = w4.get(c_off).copied().unwrap_or(0);
            println!("  Alignment Check (Axiom 0367): Offset {} -> Raw Class {}", c_off, raw_class);
            if raw_class != merc.class_id {
                println!("  [WARN] Alignment Drift Detected! MercenaryState class ({}) != raw class ({})", merc.class_id, raw_class);
            }

        } else {
            println!("w4 section NOT found");
        }
        println!("Header XP: {}", u32::from_le_bytes(bytes[171..175].try_into()?));
        println!();
    }

    Ok(())
}
