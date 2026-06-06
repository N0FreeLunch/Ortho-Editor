// This software is licensed under the PolyForm Noncommercial License 1.0.0.
// Required Notice: Copyright 2026 N0FreeLunch (https://github.com/N0FreeLunch/d2r-core)

use d2r_core::domain::progression::axiom::{
    PROG_START_FILE, V105_QUEST_OFFSET, V105_WAYPOINT_OFFSET, V105_NPC_OFFSET,
};
use d2r_core::save::map_core_sections;
use d2r_core::verify::args::{ArgError, ArgParser, ArgSpec};
use serde::Serialize;
use std::env;
use std::fs;
use std::process;

#[derive(Serialize)]
struct ProgressionReport {
    fixture_path: String,
    axioms: Axioms,
    actual: Actual,
    matches: Matches,
    overall_pass: bool,
}

#[derive(Serialize)]
struct Axioms {
    prog_start_file: usize,
    v105_quest_offset: usize,
    v105_waypoint_offset: usize,
    v105_npc_offset: usize,
}

#[derive(Serialize)]
struct Actual {
    woo_pos: Option<usize>,
    ws_pos: Option<usize>,
    w4_pos: Option<usize>,
    gf_pos: usize,
}

#[derive(Serialize)]
struct Matches {
    woo_match: bool,
    ws_match: bool,
    w4_match: bool,
}

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

    let is_json = parsed.is_json();
    let file_path = parsed.get("fixture").unwrap();

    let bytes = match fs::read(file_path) {
        Ok(b) => b,
        Err(e) => {
            if is_json {
                // For JSON mode, we could still output a failure object, but let's keep it simple for now
                eprintln!("[ERROR] Failed to read file {}: {}", file_path, e);
            } else {
                eprintln!("[ERROR] Failed to read file {}: {}", file_path, e);
            }
            process::exit(1);
        }
    };

    let map = match map_core_sections(&bytes) {
        Ok(m) => m,
        Err(e) => {
            if is_json {
                eprintln!("[FATAL] Failed to map core sections: {}", e);
            } else {
                eprintln!("[FATAL] Failed to map core sections: {}", e);
            }
            process::exit(1);
        }
    };

    let woo_match = map.woo_pos == Some(V105_QUEST_OFFSET);
    let ws_match = map.ws_pos == Some(V105_WAYPOINT_OFFSET);
    let w4_match = map.w4_pos == Some(V105_NPC_OFFSET);
    let overall_pass = woo_match && ws_match && w4_match;

    if is_json {
        let report = ProgressionReport {
            fixture_path: file_path.clone(),
            axioms: Axioms {
                prog_start_file: PROG_START_FILE,
                v105_quest_offset: V105_QUEST_OFFSET,
                v105_waypoint_offset: V105_WAYPOINT_OFFSET,
                v105_npc_offset: V105_NPC_OFFSET,
            },
            actual: Actual {
                woo_pos: map.woo_pos,
                ws_pos: map.ws_pos,
                w4_pos: map.w4_pos,
                gf_pos: map.gf_pos,
            },
            matches: Matches {
                woo_match,
                ws_match,
                w4_match,
            },
            overall_pass,
        };

        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        if !overall_pass {
            process::exit(1);
        }
        return;
    }

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
            let match_str = if woo_match { "PASS" } else { failures += 1; "FAIL" };
            println!("  'Woo!' (Quest)     : 0x{:04X} (rel {}) -> [{}]", pos, rel, match_str);
            if !woo_match {
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
            let match_str = if ws_match { "PASS" } else { failures += 1; "FAIL" };
            println!("  'WS'   (Waypoint)  : 0x{:04X} (rel {}) -> [{}]", pos, rel, match_str);
            if !ws_match {
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
            let match_str = if w4_match { "PASS" } else { failures += 1; "FAIL" };
            println!("  'w4'   (NPC/Exp)   : 0x{:04X} (rel {}) -> [{}]", pos, rel, match_str);
            if !w4_match {
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
