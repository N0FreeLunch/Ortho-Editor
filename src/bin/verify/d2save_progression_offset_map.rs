// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use d2r_core::domain::progression::axiom::{
    PROG_START_FILE, V105_QUEST_OFFSET, V105_WAYPOINT_OFFSET, V105_NPC_OFFSET,
};
use d2r_core::save::map_core_sections;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use std::env;
use std::fs;
use std::process;

fn main() {
    let mut parser = ArgParser::new("d2save_progression_offset_map");
    parser.add_spec(ArgSpec::positional("fixture", "Path to Alpha v105 save file"));

    let parsed = match parser.parse(env::args_os().skip(1).collect()) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("error: {}", e);
            eprintln!("\n{}", parser.usage());
            process::exit(1);
        }
    };

    let file_path = parsed.get("fixture").unwrap();

    let bytes = match fs::read(file_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[ERROR] Failed to read file {}: {}", file_path, e);
            process::exit(1);
        }
    };

    let map = match map_core_sections(&bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[FATAL] Failed to map core sections: {}", e);
            process::exit(1);
        }
    };

    println!("--- Alpha v105 Progression Offset Mapping Verification ---");
    println!("Target Fixture: {}", file_path);
    println!("");

    println!("Axiom Constants (Truth):");
    println!("  PROG_START_FILE    : 0x{:04X} ({})", PROG_START_FILE, PROG_START_FILE);
    println!("  V105_QUEST_OFFSET  : 0x{:04X} ({})", V105_QUEST_OFFSET, V105_QUEST_OFFSET);
    println!("  V105_WAYPOINT_OFFSET: 0x{:04X} ({})", V105_WAYPOINT_OFFSET, V105_WAYPOINT_OFFSET);
    println!("  V105_NPC_OFFSET    : 0x{:04X} ({})", V105_NPC_OFFSET, V105_NPC_OFFSET);
    println!("");

    let mut failures = 0;

    println!("Actual Marker Positions & Progression-Relative Offsets:");
    
    // Woo! (Quest)
    match map.woo_pos {
        Some(pos) => {
            let rel = pos as isize - PROG_START_FILE as isize;
            let match_str = if pos == V105_QUEST_OFFSET { "PASS" } else { failures += 1; "FAIL" };
            println!("  'Woo!' (Quest)     : 0x{:04X} (rel {}) -> [{}]", pos, rel, match_str);
            if pos != V105_QUEST_OFFSET {
                println!("    Mismatch: Expected 0x{:04X}", V105_QUEST_OFFSET);
            }
        }
        None => {
            println!("  'Woo!' (Quest)     : NOT FOUND -> [FAIL]");
            failures += 1;
        }
    }

    // WS (Waypoint)
    match map.ws_pos {
        Some(pos) => {
            let rel = pos as isize - PROG_START_FILE as isize;
            let match_str = if pos == V105_WAYPOINT_OFFSET { "PASS" } else { failures += 1; "FAIL" };
            println!("  'WS'   (Waypoint)  : 0x{:04X} (rel {}) -> [{}]", pos, rel, match_str);
            if pos != V105_WAYPOINT_OFFSET {
                println!("    Mismatch: Expected 0x{:04X}", V105_WAYPOINT_OFFSET);
            }
        }
        None => {
            println!("  'WS'   (Waypoint)  : NOT FOUND -> [FAIL]");
            failures += 1;
        }
    }

    // w4 (NPC/Expansion)
    match map.w4_pos {
        Some(pos) => {
            let rel = pos as isize - PROG_START_FILE as isize;
            let match_str = if pos == V105_NPC_OFFSET { "PASS" } else { failures += 1; "FAIL" };
            println!("  'w4'   (NPC/Exp)   : 0x{:04X} (rel {}) -> [{}]", pos, rel, match_str);
            if pos != V105_NPC_OFFSET {
                println!("    Mismatch: Expected 0x{:04X}", V105_NPC_OFFSET);
            }
        }
        None => {
            println!("  'w4'   (NPC/Exp)   : NOT FOUND -> [FAIL]");
            failures += 1;
        }
    }

    // gf (Stats) - Informative
    let gf_rel = map.gf_pos as isize - PROG_START_FILE as isize;
    println!("  'gf'   (Stats)     : 0x{:04X} (rel {}) -> [INFO]", map.gf_pos, gf_rel);

    println!("");
    if failures > 0 {
        println!("Verification FAILED with {} mismatch(es).", failures);
        process::exit(1);
    } else {
        println!("Verification PASSED (All markers match Alpha v105 axioms).");
    }
}
