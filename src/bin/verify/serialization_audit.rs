use d2r_core::domain::stats::axiom::StatsAxiom;
use d2r_core::item::serialization::D2ItemSerializer;
use d2r_core::save::D2Save;
use std::env;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: d2item_serialization_audit <path_to_d2s>");
        return Ok(());
    }

    let d2s_path = &args[1];
    let data = fs::read(d2s_path)?;
    let mut save = D2Save::from_bytes(&data)?;

    println!("Serialization Audit for: {}", d2s_path);
    println!("--------------------------------------------------------------------------------");
    println!("  Idx | Code       |  OrigLen |   SerLen | Match | Fid  ");
    println!("------------------------------------------------------------------------------------------");

    for (idx, item) in save.items.iter().enumerate() {
        let axiom = StatsAxiom::new(save.header.version, true);
        
        // Original bits from save data
        let orig_bits = item.raw_bits();
        let orig_len = orig_bits.len();

        // Reserialize
        let mut serializer = D2ItemSerializer::new(axiom.clone());
        let ser_bits = serializer.write_item(item)?;
        let ser_len = ser_bits.len();

        let is_match = orig_bits == ser_bits;
        let match_str = if is_match { "OK" } else { "FAIL" };
        
        let mut fidelity = 1.0;
        if !is_match {
            fidelity = 0.4; // Default penalty for mismatch
        }

        println!(
            "{:>5} | {:<10} | {:>8} | {:>8} | {:<5} | {:.2}",
            idx, item.code, orig_len, ser_len, match_str, fidelity
        );

        if !is_match {
            if orig_len != ser_len {
                println!("      [REASON] Length");
            } else {
                println!("      [REASON] Content");
            }
            
            // Find first mismatch bit
            let min_len = std::cmp::min(orig_len, ser_len);
            let mut first_diff = None;
            for i in 0..min_len {
                if orig_bits[i] != ser_bits[i] {
                    first_diff = Some(i);
                    break;
                }
            }
            
            if let Some(pos) = first_diff {
                println!("      [OFFSET] bit {}", pos);
            }
        }
        
        if item.is_alpha_v105_shadow {
             println!("      [FORENSIC RATIONALE]");
             println!("        - [EmergingHypothesis] Variable gap between JM header and item body in Alpha v105");
        }
    }

    println!("------------------------------------------------------------------------------------------");
    
    let all_match = save.items.iter().all(|it| {
        let axiom = StatsAxiom::new(save.header.version, true);
        let orig = it.raw_bits();
        let mut serializer = D2ItemSerializer::new(axiom);
        let ser = serializer.write_item(it).unwrap_or_default();
        orig == ser
    });

    if all_match {
        println!("SUCCESS: All items match bit-perfectly.");
    } else {
        println!("FAIL: Mismatches detected.");
        std::process::exit(1);
    }

    Ok(())
}
