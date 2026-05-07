use std::env;
use std::fs;
use std::process;
use d2r_core::domain::progression::Progression;
use d2r_core::domain::progression::axiom::{V105_QUEST_OFFSET, V105_QUEST_LEN, V105_WAYPOINT_OFFSET, V105_WAYPOINT_LEN};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <fixture.d2s> [--semantic]", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];
    let semantic_mode = args.iter().any(|arg| arg == "--semantic");

    let original_bytes = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Error reading file {}: {}", file_path, e);
            process::exit(1);
        }
    };

    println!("--- Alpha v105 Progression Parity Audit ---");
    println!("File: {}", file_path);
    if semantic_mode {
        println!("Mode: Semantic Report");
    }

    let mut buffer = original_bytes.clone();
    
    // We assume Alpha mode as this is for Alpha v105 verification
    let result = Progression::from_bytes(&original_bytes, true);
    let progression = match result.value {
        Ok(p) => p,
        Err(e) => {
            eprintln!("  [FAIL] Failed to parse progression: {}", e);
            process::exit(1);
        }
    };

    if semantic_mode {
        println!("\n[QUEST SEMANTIC STATUS]");
        for quest in progression.quests.quests() {
            let status = if quest.is_completed() { "COMPLETED" } else { "pending" };
            println!("  - {:<30} : {}", quest.name(), status);
        }

        println!("\n[WAYPOINT SEMANTIC STATUS]");
        for wp in progression.waypoints.waypoints() {
            let status = if wp.is_active() { "ACTIVE" } else { "locked" };
            println!("  - {:<30} : {}", wp.name(), status);
        }
        println!("\n--- End of Semantic Report ---\n");
    }

    progression.sync_to_bytes(&mut buffer, true);

    let mut failures = 0;

    // 1. Quest Parity
    let quest_range = V105_QUEST_OFFSET..(V105_QUEST_OFFSET + V105_QUEST_LEN);
    if buffer.len() >= quest_range.end && original_bytes.len() >= quest_range.end {
        let original_quest = &original_bytes[quest_range.clone()];
        let synced_quest = &buffer[quest_range.clone()];
        
        if original_quest == synced_quest {
            println!("  [PASS] Quest Section Parity");
        } else {
            println!("  [FAIL] Quest Section Parity Mismatch");
            for i in 0..V105_QUEST_LEN {
                if original_quest[i] != synced_quest[i] {
                    println!("    First mismatch at offset 0x{:04X} (rel 0x{:X}): expected 0x{:02X}, got 0x{:02X}", 
                        V105_QUEST_OFFSET + i, i, original_quest[i], synced_quest[i]);
                    break;
                }
            }
            failures += 1;
        }
    } else {
        println!("  [FAIL] Quest Section: buffer too small (len={}, required={})", buffer.len(), quest_range.end);
        failures += 1;
    }

    // 2. Waypoint Parity
    let wp_range = V105_WAYPOINT_OFFSET..(V105_WAYPOINT_OFFSET + V105_WAYPOINT_LEN);
    if buffer.len() >= wp_range.end && original_bytes.len() >= wp_range.end {
        let original_wp = &original_bytes[wp_range.clone()];
        let synced_wp = &buffer[wp_range.clone()];
        
        if original_wp == synced_wp {
            println!("  [PASS] Waypoint Section Parity");
        } else {
            println!("  [FAIL] Waypoint Section Parity Mismatch");
             for i in 0..V105_WAYPOINT_LEN {
                if original_wp[i] != synced_wp[i] {
                    println!("    First mismatch at offset 0x{:04X} (rel 0x{:X}): expected 0x{:02X}, got 0x{:02X}", 
                        V105_WAYPOINT_OFFSET + i, i, original_wp[i], synced_wp[i]);
                    break;
                }
            }
            failures += 1;
        }
    } else {
        println!("  [FAIL] Waypoint Section: buffer too small (len={}, required={})", buffer.len(), wp_range.end);
        failures += 1;
    }

    if failures > 0 {
        println!("\nProgression Audit FAILED with {} section mismatch(es).", failures);
        process::exit(1);
    } else {
        println!("\nProgression Audit PASSED.");
    }
}
