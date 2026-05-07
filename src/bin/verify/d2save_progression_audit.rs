use std::env;
use std::fs;
use std::process;
use d2r_core::domain::progression::Progression;
use d2r_core::domain::progression::axiom::{V105_QUEST_OFFSET, V105_QUEST_LEN, V105_WAYPOINT_OFFSET, V105_WAYPOINT_LEN};

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.iter().any(|arg| arg == "--help" || arg == "-h") || args.len() < 2 {
        println!("D2R Alpha v105 Progression Parity Audit Tool");
        println!("\nUsage: {} <fixture.d2s> [options]", args[0]);
        println!("\nOptions:");
        println!("  --semantic                    Print semantic status of quests and waypoints");
        println!("  --mutate <domain:target=state,...> Apply mutations before parity check (comma-separated)");
        println!("                                Domains: quest (default), waypoint/wp");
        println!("                                States: completed/pending (quest), active/locked (waypoint)");
        println!("                                (e.g., quest:Sisters_to_the_Slaughter=completed,wp:Act_1_-_Town=active)");
        println!("  --help, -h                    Print this help message");
        println!("\nExit Codes:");
        println!("  0  Audit Passed (100% Bit Parity)");
        println!("  1  Audit Failed or CLI Error");
        process::exit(if args.len() < 2 { 1 } else { 0 });
    }

    let file_path = &args[1];
    let semantic_mode = args.iter().any(|arg| arg == "--semantic");
    
    let mutate_opt = args.iter().position(|arg| arg == "--mutate").and_then(|i| {
        let val = args.get(i + 1);
        if val.is_none() {
            eprintln!("[ERROR] --mutate requires a value in 'domain:target=state' format.");
            process::exit(1);
        }
        val
    });

    let original_bytes = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[ERROR] Failed to read file {}: {}", file_path, e);
            process::exit(1);
        }
    };

    println!("--- Alpha v105 Progression Parity Audit ---");
    println!("Target Fixture: {}", file_path);
    
    if let Some(m) = mutate_opt {
        println!("Operation Mode: Mutation Audit ({})", m);
    } else {
        println!("Operation Mode: Baseline Parity Audit");
    }

    // Parse progression
    let result = Progression::from_bytes(&original_bytes, true);
    let mut progression = match result.value {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[FATAL] Failed to parse progression section: {}", e);
            process::exit(1);
        }
    };

    // Apply mutation if requested
    if let Some(target_str) = mutate_opt {
        for part in target_str.split(',') {
            match parse_mutation_input(part) {
                Ok((domain, name, state)) => {
                    let normalized_name = name.replace('_', " ");
                    match domain.as_str() {
                        "quest" => {
                            if let Some(q) = progression.quests.quests_mut().iter_mut().find(|q| q.name().eq_ignore_ascii_case(&normalized_name)) {
                                let completed = state == "completed" || state == "true" || state == "1" || state == "active";
                                println!("  [MUTATE] Quest '{}' -> {}", q.name(), if completed { "COMPLETED" } else { "PENDING" });
                                q.set_completed(completed);
                            } else {
                                eprintln!("[ERROR] Quest '{}' not found in progression table.", name);
                                process::exit(1);
                            }
                        },
                        "waypoint" | "wp" => {
                            if let Some(w) = progression.waypoints.waypoints_mut().iter_mut().find(|w| w.name().eq_ignore_ascii_case(&normalized_name)) {
                                let active = state == "active" || state == "true" || state == "1" || state == "completed";
                                println!("  [MUTATE] Waypoint '{}' -> {}", w.name(), if active { "ACTIVE" } else { "LOCKED" });
                                w.set_active(active);
                            } else {
                                eprintln!("[ERROR] Waypoint '{}' not found in progression table.", name);
                                process::exit(1);
                            }
                        },
                        _ => {
                            eprintln!("[ERROR] Unknown domain '{}'. Use 'quest' or 'waypoint'.", domain);
                            process::exit(1);
                        }
                    }
                },
                Err(e) => {
                    eprintln!("[ERROR] Invalid mutation format in part '{}': {}", part, e);
                    process::exit(1);
                }
            }
        }
    }

    if semantic_mode {
        println!("\n[SEMANTIC REPORT: QUESTS]");
        for quest in progression.quests.quests() {
            let status = if quest.is_completed() { "COMPLETED" } else { "pending" };
            println!("  - {:<35} : {}", quest.name(), status);
        }

        println!("\n[SEMANTIC REPORT: WAYPOINTS]");
        for wp in progression.waypoints.waypoints() {
            let status = if wp.is_active() { "ACTIVE" } else { "locked" };
            println!("  - {:<35} : {}", wp.name(), status);
        }
        println!("\n--- End of Semantic Report ---\n");
    }

    let mut buffer = original_bytes.clone();
    progression.sync_to_bytes(&mut buffer, true);

    // Parity Verification logic
    let (expected_bytes, actual_bytes) = if mutate_opt.is_some() {
        println!("\n[VERIFICATION: ROUND-TRIP PARITY]");
        let mutated_baseline = buffer.clone();
        let second_result = Progression::from_bytes(&mutated_baseline, true);
        let second_progression = match second_result.value {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[FATAL] Round-trip re-parse failed: {}", e);
                process::exit(1);
            }
        };
        let mut second_buffer = mutated_baseline.clone();
        second_progression.sync_to_bytes(&mut second_buffer, true);
        (mutated_baseline, second_buffer)
    } else {
        println!("\n[VERIFICATION: BASELINE SYNC PARITY]");
        (original_bytes, buffer)
    };

    let mut failures = 0;

    // 1. Quest Parity
    let quest_range = V105_QUEST_OFFSET..(V105_QUEST_OFFSET + V105_QUEST_LEN);
    if actual_bytes.len() >= quest_range.end && expected_bytes.len() >= quest_range.end {
        let expected_quest = &expected_bytes[quest_range.clone()];
        let actual_quest = &actual_bytes[quest_range.clone()];
        
        if expected_quest == actual_quest {
            println!("  [PASS] Quest Section Parity (round-trip stable)");
        } else {
            println!("  [FAIL] Quest Section Parity Mismatch");
            for i in 0..V105_QUEST_LEN {
                if expected_quest[i] != actual_quest[i] {
                    println!("    First mismatch at offset 0x{:04X} (rel 0x{:X}): expected 0x{:02X}, got 0x{:02X}", 
                        V105_QUEST_OFFSET + i, i, expected_quest[i], actual_quest[i]);
                    break;
                }
            }
            failures += 1;
        }
    } else {
        println!("  [FAIL] Quest Section: buffer size mismatch (len={}, required={})", actual_bytes.len(), quest_range.end);
        failures += 1;
    }

    // 2. Waypoint Parity
    let wp_range = V105_WAYPOINT_OFFSET..(V105_WAYPOINT_OFFSET + V105_WAYPOINT_LEN);
    if actual_bytes.len() >= wp_range.end && expected_bytes.len() >= wp_range.end {
        let expected_wp = &expected_bytes[wp_range.clone()];
        let actual_wp = &actual_bytes[wp_range.clone()];
        
        if expected_wp == actual_wp {
            println!("  [PASS] Waypoint Section Parity (round-trip stable)");
        } else {
            println!("  [FAIL] Waypoint Section Parity Mismatch");
             for i in 0..V105_WAYPOINT_LEN {
                if expected_wp[i] != actual_wp[i] {
                    println!("    First mismatch at offset 0x{:04X} (rel 0x{:X}): expected 0x{:02X}, got 0x{:02X}", 
                        V105_WAYPOINT_OFFSET + i, i, expected_wp[i], actual_wp[i]);
                    break;
                }
            }
            failures += 1;
        }
    } else {
        println!("  [FAIL] Waypoint Section: buffer size mismatch (len={}, required={})", actual_bytes.len(), wp_range.end);
        failures += 1;
    }

    if failures > 0 {
        println!("\nProgression Audit FAILED with {} section mismatch(es).", failures);
        process::exit(1);
    } else {
        println!("\nProgression Audit PASSED (100% bit-parity).");
    }
}

fn parse_mutation_input(input: &str) -> Result<(String, String, String), String> {
    if let Some((domain_part, state)) = input.split_once('=') {
        if state.trim().is_empty() {
            return Err("Target state cannot be empty".to_string());
        }
        
        let (domain, name) = if let Some((d, n)) = domain_part.split_once(':') {
            (d.to_lowercase(), n.to_string())
        } else {
            ("quest".to_string(), domain_part.to_string())
        };

        if name.trim().is_empty() {
            return Err("Target name cannot be empty".to_string());
        }
        
        Ok((domain, name, state.to_lowercase()))
    } else {
        Err("Expected format 'domain:name=state' (e.g., quest:Sisters_to_the_Slaughter=completed)".to_string())
    }
}


