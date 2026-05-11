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
            println!("w4 Marker: {:02X?}", &w4[0..2]);
            println!("w4 Bytes (first 64): {:02X?}", &w4[..w4.len().min(64)]);
            
            // Re-decode using hybrid logic
            let w4_payload = &w4[2..]; // Skip 'w4'
            let merc = MercenaryState::from_hybrid(&bytes, Some(w4_payload));
            println!("Hybrid Decoded: ID={}, Class={}, Subtype={}, XP={}, NameID={}", 
                merc.hireling_id, merc.class_id, merc.subtype_id, merc.experience, merc.name_id);
        } else {
            println!("w4 section NOT found");
        }
        println!("Header XP: {}", u32::from_le_bytes(bytes[171..175].try_into()?));
        println!();
    }

    Ok(())
}
