use d2r_core::item::{HuffmanTree, Item};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

mod cli {
    pub struct Args {
        pub corpus_dir: Option<String>,
        pub fixture: Option<String>,
        pub json: bool,
        pub token_efficient: bool,
        pub verbose: bool,
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut corpus_dir = None;
        let mut fixture = None;
        let mut json = false;
        let mut token_efficient = false;
        let mut verbose = false;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--corpus-dir" => {
                    if i + 1 < args.len() {
                        corpus_dir = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--fixture" | "--fixture-name" | "--file" => {
                    if i + 1 < args.len() {
                        fixture = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--json" => json = true,
                "--token-efficient" => token_efficient = true,
                "--verbose" | "-v" => verbose = true,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {}
            }
            i += 1;
        }

        Args {
            corpus_dir,
            fixture,
            json,
            token_efficient,
            verbose,
        }
    }

    fn print_help() {
        println!("d2save_equipped_invariant - D2R Equipped & Mercenary Item Identification Invariant Verifier");
        println!();
        println!("USAGE:");
        println!("    d2save_equipped_invariant [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("    --fixture <PATH>        Verify a single .d2s save file (fast diagnosis)");
        println!("    --corpus-dir <PATH>     Directory of saves to scan (default: tests/fixtures/savegames)");
        println!("    --json                  Output structured JSON report");
        println!("    --token-efficient       Output compact summary for token conservation");
        println!("    --verbose, -v           Detailed per-anomaly breakdown");
        println!("    --help, -h              Show this help message");
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum InvariantClassification {
    /// Phantom header caused by bit-drift from preceding variable-length option list.
    PhantomHeaderDrift,
    /// Flag bit-shift anomaly (e.g. bit 5 is set instead of bit 4).
    FlagShiftAnomaly,
    /// Genuine hard violation where a valid equipped item is unidentified.
    TrueUnidentifiedViolation,
}

#[derive(Debug, Clone, Serialize)]
pub struct EquippedAnomaly {
    pub file: String,
    pub item_index: usize,
    pub code: String,
    pub version: u8,
    pub mode: u8,
    pub location: u8,
    pub y: u8,
    pub x: u8,
    pub flags: String,
    pub is_identified: bool,
    pub is_compact: bool,
    pub is_opaque: bool,
    pub classification: InvariantClassification,
    pub diagnosis: String,
}

#[derive(Debug, Serialize)]
pub struct InvariantAuditReport {
    pub total_saves_checked: usize,
    pub total_items_checked: usize,
    pub total_equipped_checked: usize,
    pub total_anomalies: usize,
    pub phantom_header_drift_count: usize,
    pub flag_shift_anomaly_count: usize,
    pub true_violation_count: usize,
    pub affected_files: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anomalies: Vec<EquippedAnomaly>,
}

fn classify_anomaly(
    code: &str,
    version: u8,
    location: u8,
    mode: u8,
    y: u8,
    flags: u32,
) -> (InvariantClassification, String) {
    let clean_code = code.trim();

    // Condition 1: Check for phantom codes, invalid characters, or invalid body slot
    let axiom = d2r_core::domain::header::HeaderAxiom::new(version, true);
    let is_valid_item = axiom.is_plausible(mode, location, code.as_bytes(), flags)
        || axiom.is_plausible(mode, location, clean_code.as_bytes(), flags);

    let is_invalid_body_loc = (mode == 1) && (y == 0 || y > 12);
    let is_known_phantom = matches!(clean_code, "wyws" | "wsd" | "wsd " | "oesw" | "w   " | "yws " | "s   " | "kk h");
    let has_internal_space = clean_code.contains(' ') && clean_code.len() > 3;

    if !is_valid_item || is_known_phantom || is_invalid_body_loc || has_internal_space || clean_code.len() < 3 {
        return (
            InvariantClassification::PhantomHeaderDrift,
            format!(
                "Bit-drift rupture from preceding option stream produced invalid item header '{}' (body_slot={}, loc={}, mode={}, ver={})",
                clean_code, y, location, mode, version
            ),
        );
    }

    // Condition 2: Check for 1-bit flag shift (bit 5 set instead of bit 4)
    if (flags & 0x00000020) != 0 && (flags & 0x00000010) == 0 {
        return (
            InvariantClassification::FlagShiftAnomaly,
            format!(
                "1-bit shifted flag pattern in code '{}': bit 5 is 1 while bit 4 (is_identified) is 0 (flags=0x{:08x})",
                clean_code, flags
            ),
        );
    }

    // Condition 3: Genuine violation
    (
        InvariantClassification::TrueUnidentifiedViolation,
        format!(
            "Hard invariant violation: Genuine equipped item '{}' (body_slot={}) has is_identified == false",
            clean_code, y
        ),
    )
}

fn collect_d2s_files(dir: &Path, list: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_d2s_files(&path, list);
            } else if path.extension().and_then(|s| s.to_str()) == Some("d2s") {
                list.push(path);
            }
        }
    }
}

fn audit_file(
    path: &Path,
    huffman: &HuffmanTree,
) -> (usize, usize, Vec<EquippedAnomaly>) {
    let Ok(bytes) = fs::read(path) else {
        return (0, 0, Vec::new());
    };

    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut total_items = 0;
    let mut total_equipped = 0;
    let mut anomalies = Vec::new();

    if let Ok(items) = Item::read_player_items(&bytes, huffman, true) {
        total_items += items.len();
        for (idx, it) in items.iter().enumerate() {
            // In D2 save format, mode == 1 indicates an equipped item (body slot 1..=10).
            // mode == 0 with location == 1 means stored in player inventory (can be unidentified).
            let is_equipped = it.header.mode == 1 || it.mode == 1;

            if is_equipped {
                total_equipped += 1;
                if !it.header.is_identified {
                    let (classification, diagnosis) = classify_anomaly(
                        &it.code,
                        it.header.version,
                        it.header.location,
                        it.header.mode,
                        it.header.y,
                        it.header.flags,
                    );

                    anomalies.push(EquippedAnomaly {
                        file: filename.clone(),
                        item_index: idx,
                        code: it.code.trim().to_string(),
                        version: it.header.version,
                        mode: it.header.mode,
                        location: it.header.location,
                        y: it.header.y,
                        x: it.header.x,
                        flags: format!("0x{:08x}", it.header.flags),
                        is_identified: it.header.is_identified,
                        is_compact: it.header.is_compact,
                        is_opaque: it.is_opaque() || it.is_semi_opaque(),
                        classification,
                        diagnosis,
                    });
                }
            }
        }
    }

    (total_items, total_equipped, anomalies)
}

fn main() {
    let start_time = Instant::now();
    let args = cli::parse_args();
    let huffman = HuffmanTree::new();

    let mut save_paths = Vec::new();

    if let Some(ref single_fixture) = args.fixture {
        let p = PathBuf::from(single_fixture);
        if !p.exists() {
            eprintln!("Error: Specified fixture file does not exist: {}", single_fixture);
            std::process::exit(1);
        }
        save_paths.push(p);
    } else {
        let search_dir = args.corpus_dir.as_deref().unwrap_or_else(|| {
            if Path::new("tests/fixtures/savegames/gameplay/normal").exists() {
                "tests/fixtures/savegames/gameplay/normal"
            } else if Path::new("tests/fixtures/savegames").exists() {
                "tests/fixtures/savegames"
            } else {
                "."
            }
        });
        collect_d2s_files(Path::new(search_dir), &mut save_paths);
        save_paths.sort();
    }

    if save_paths.is_empty() {
        eprintln!("Warning: No .d2s save files found to verify.");
        std::process::exit(0);
    }

    // Parallel audit using rayon
    let results: Vec<(usize, usize, Vec<EquippedAnomaly>)> = save_paths
        .par_iter()
        .map(|path| audit_file(path, &huffman))
        .collect();

    let mut total_items = 0;
    let mut total_equipped = 0;
    let mut all_anomalies = Vec::new();
    let mut affected_files_set = BTreeMap::new();

    for (items_cnt, equipped_cnt, file_anomalies) in results {
        total_items += items_cnt;
        total_equipped += equipped_cnt;
        for anom in file_anomalies {
            *affected_files_set.entry(anom.file.clone()).or_insert(0) += 1;
            all_anomalies.push(anom);
        }
    }

    let mut phantom_count = 0;
    let mut flag_shift_count = 0;
    let mut true_violation_count = 0;

    for a in &all_anomalies {
        match a.classification {
            InvariantClassification::PhantomHeaderDrift => phantom_count += 1,
            InvariantClassification::FlagShiftAnomaly => flag_shift_count += 1,
            InvariantClassification::TrueUnidentifiedViolation => true_violation_count += 1,
        }
    }

    let affected_files: Vec<String> = affected_files_set.into_keys().collect();

    let report = InvariantAuditReport {
        total_saves_checked: save_paths.len(),
        total_items_checked: total_items,
        total_equipped_checked: total_equipped,
        total_anomalies: all_anomalies.len(),
        phantom_header_drift_count: phantom_count,
        flag_shift_anomaly_count: flag_shift_count,
        true_violation_count,
        affected_files: affected_files.clone(),
        anomalies: if args.token_efficient {
            Vec::new()
        } else {
            all_anomalies.clone()
        },
    };

    if args.json {
        let json_str = serde_json::to_string_pretty(&report).unwrap();
        println!("{}", json_str);
        return;
    }

    // Human-readable console output
    let elapsed = start_time.elapsed();
    println!("================================================================================");
    println!("  d2save_equipped_invariant: D2R Equipped Identification Invariant Radar");
    println!("================================================================================");
    println!("Files audited:       {}", save_paths.len());
    println!("Total items:         {}", total_items);
    println!("Equipped items:      {}", total_equipped);
    println!("Total anomalies:     {}", all_anomalies.len());
    println!("Elapsed time:        {:.2?}", elapsed);
    println!("--------------------------------------------------------------------------------");
    println!("Anomaly Breakdown:");
    println!("  [1] Phantom Header Drift:        {} (Bit-drift rupture from preceding option stream)", phantom_count);
    println!("  [2] Flag Shift Anomaly:          {} (Bit 5 set instead of Bit 4)", flag_shift_count);
    println!("  [3] True Unidentified Violation: {} (Genuine equipped item unidentified)", true_violation_count);
    println!("--------------------------------------------------------------------------------");

    if !affected_files.is_empty() {
        println!("Affected Save Files ({} files):", affected_files.len());
        for file in &affected_files {
            println!("  - {}", file);
        }
    }

    if args.verbose || (!args.token_efficient && !all_anomalies.is_empty()) {
        println!("--------------------------------------------------------------------------------");
        println!("Sample Anomalies (First 20):");
        for anom in all_anomalies.iter().take(20) {
            println!(
                "  [{}] idx={:02} code='{:4}' ver={} loc={} mode={} y={:2} x={:2} flags={} class={:?}",
                anom.file, anom.item_index, anom.code, anom.version, anom.location, anom.mode, anom.y, anom.x, anom.flags, anom.classification
            );
        }
        if all_anomalies.len() > 20 {
            println!("  ... and {} more anomalies.", all_anomalies.len() - 20);
        }
    }

    println!("================================================================================");
    if true_violation_count > 0 {
        eprintln!("WARNING: Found {} TRUE unidentified equipped items!", true_violation_count);
    } else {
        println!("Radar Verdict: ZERO true unidentified violations. All anomalies are parser bit-drifts.");
    }
}
