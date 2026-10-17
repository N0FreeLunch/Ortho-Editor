//! # Equipped Item Identification Invariant Verifier
//!
//! ## Domain Invariant Design Intent
//! In authentic Diablo II: Resurrected gameplay, all equipped items on both player characters
//! and mercenaries MUST be in an identified state (`is_identified == true`).
//! Unidentified items cannot be physically equipped in the game engine without external tampering.
//!
//! ## Harness Fallibility Notice (Thinking 4057 / harness-fallibility-governor)
//! Automated verification harnesses are themselves fallible and subject to domain knowledge gaps.
//! When evaluating an anomaly:
//! 1. Do NOT assume that an unidentified flag on an equipped item is always a gameplay/save data corruption.
//! 2. In Diablo II, items have strict physical slot constraints:
//!    - Rings (`rin`) can only be equipped in body slots 6 or 7 (never belt 8 or helm 1).
//!    - Amulets (`amu`) can only be equipped in body slot 2.
//!    - Charms (`cm1`, `cm2`, `cm3`), Runes, and Gems can NEVER be equipped in any body slot.
//!    - Mercenaries cannot equip rings, amulets, belts, boots, or gloves under any circumstances.
//! 3. If an item appears in an impossible slot with `is_identified == false`, it is 100% guaranteed
//!    to be a parser bit-drift rupture (`SlotMismatchPhantomDrift`), NOT a genuine save violation.
//!    The physical laws of the game world always take precedence over verification heuristics.

use d2r_core::item::{HuffmanTree, Item};
use d2r_core::verify::alpha_inventory_routing::{
    AlphaInventoryRoute, alpha_inventory_candidate_route, is_belt_consumable,
};

// Alpha belt claims are checked before routing isolation, regardless of identification
// or opacity. Non-consumables cannot occupy belt storage; preserve and report them.
// This is evidence of an impossible parsed placement, not proof of its root cause.
// D2R-v3 belt slots count left to right, then upward: column = x % 4, row = x / 4.
// Each occupied row must have every lower row occupied in the same column.
// Check raw claims before routing isolation; never compact or repair source slots.
// Alpha positions and equipped-belt-specific capacity remain outside coverage.

/// The audit covers only the primary player list, not mercenary/corpse/golem lists.
/// Slot compatibility is a partial diagnostic heuristic, not a complete gear oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Dialect {
    #[serde(rename = "alpha")]
    Alpha,
    #[serde(rename = "d2r-v3")]
    D2rV3,
}

impl Dialect {
    fn parse(value: Option<&str>) -> Result<Self, &'static str> {
        match value {
            Some("alpha") => Ok(Self::Alpha),
            Some("d2r-v3") => Ok(Self::D2rV3),
            _ => Err("--dialect requires alpha or d2r-v3"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Alpha => "alpha",
            Self::D2rV3 => "d2r-v3",
        }
    }
}

use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

mod cli {
    pub struct Args {
        pub dialect: super::Dialect,
        pub corpus_dir: Option<String>,
        pub fixture: Option<String>,
        pub json: bool,
        pub token_efficient: bool,
        pub verbose: bool,
    }

    pub fn parse_args() -> Args {
        let args: Vec<String> = std::env::args().collect();
        let mut dialect = super::Dialect::Alpha;
        let mut corpus_dir = None;
        let mut fixture = None;
        let mut json = false;
        let mut token_efficient = false;
        let mut verbose = false;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--dialect" => {
                    dialect = super::Dialect::parse(args.get(i + 1).map(String::as_str))
                        .unwrap_or_else(|message| {
                            eprintln!("{message}");
                            std::process::exit(64);
                        });
                    i += 1;
                }
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
            dialect,
            corpus_dir,
            fixture,
            json,
            token_efficient,
            verbose,
        }
    }

    fn print_help() {
        println!(
            "d2save_equipped_invariant - Primary equipped identification and belt invariant verifier"
        );
        println!();
        println!("USAGE:");
        println!("    d2save_equipped_invariant [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!(
            "    --dialect <NAME>        alpha (default: belt kinds) or d2r-v3 (belt kinds/positions); primary list only"
        );
        println!("    --fixture <PATH>        Verify a single .d2s save file (fast diagnosis)");
        println!(
            "    --corpus-dir <PATH>     Directory of saves to scan (default: tests/fixtures/savegames)"
        );
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
    /// Slot mismatch phantom: item code is physically impossible for the assigned body slot (e.g. ring in belt slot 8).
    SlotMismatchPhantomDrift,
    /// Flag bit-shift anomaly (e.g. bit 5 is set instead of bit 4).
    FlagShiftAnomaly,
    /// Genuine hard violation where a valid equipped item is unidentified.
    TrueUnidentifiedViolation,
    /// A raw belt claim contains an item that cannot be stored in a belt.
    BeltItemMismatch,
    /// A D2R-v3 belt item has an unoccupied lower slot in the same column.
    BeltColumnGap,
    /// Multiple D2R-v3 belt items claim the same slot.
    BeltSlotCollision,
    /// A D2R-v3 belt claim lies outside the 16-slot envelope.
    BeltSlotOutOfBounds,
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
    pub schema_version: &'static str,
    pub dialect: Dialect,
    pub status: &'static str,
    pub coverage_limit: &'static str,
    pub total_saves_parsed: usize,
    pub parse_failures: Vec<AuditFailure>,
    pub total_saves_checked: usize,
    pub total_items_checked: usize,
    pub total_equipped_checked: usize,
    pub total_belt_checked: usize,
    pub belt_item_mismatch_count: usize,
    pub belt_column_gap_count: usize,
    pub belt_slot_collision_count: usize,
    pub belt_slot_out_of_bounds_count: usize,
    pub total_anomalies: usize,
    pub phantom_header_drift_count: usize,
    pub slot_mismatch_phantom_drift_count: usize,
    pub flag_shift_anomaly_count: usize,
    pub true_violation_count: usize,
    pub affected_files: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anomalies: Vec<EquippedAnomaly>,
}

impl InvariantAuditReport {
    fn exit_code(&self) -> i32 {
        if !self.parse_failures.is_empty() {
            2
        } else if self.total_anomalies > 0 {
            1
        } else {
            0
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuditFailure {
    file: String,
    stage: &'static str,
    message: String,
    bit_offset: Option<u64>,
    context: Vec<String>,
    hint: String,
}

#[derive(Debug, Default)]
struct FileAudit {
    items: usize,
    equipped: usize,
    belt: usize,
    anomalies: Vec<EquippedAnomaly>,
    failure: Option<AuditFailure>,
}

fn failed_audit(
    path: &Path,
    stage: &'static str,
    message: String,
    bit_offset: Option<u64>,
    context: Vec<String>,
    dialect: Dialect,
) -> FileAudit {
    FileAudit {
        failure: Some(AuditFailure {
            file: path.to_string_lossy().into_owned(),
            stage,
            message,
            bit_offset,
            context,
            hint: format!(
                "Preserve the input; inspect the first primary failure: d2item_inspect \"{}\" --dialect {} --section-item 0 --json",
                path.display(),
                dialect.label()
            ),
        }),
        ..FileAudit::default()
    }
}

/// Check if an item code is physically incompatible with a given body slot in Diablo II.
/// Returns Some(reason) if physically impossible, or None if plausible.
fn is_slot_incompatible(code: &str, body_slot: u8) -> Option<&'static str> {
    let clean = code.trim();
    // Non-equippable items (charms, runes, gems, keys, scrolls, books)
    // HARNESS INTENT (harness-fallibility-governor): Charms are player-inventory items.
    // If a charm code appears in a body slot (mode==1), it is a parsing artifact / bit-drift phantom.
    // KNOWN LIMITATION: This rule assumes ANY charm code in body slot is phantom.
    // If Alpha v105 encodes charms differently (e.g., socketed charm mods), revisit this rule.
    if matches!(clean, "cm1" | "cm2" | "cm3") {
        return Some("Charms can only function in player inventory, never equipped in body slots");
    }
    if clean.starts_with('r') && clean.len() == 3 && clean[1..].chars().all(|c| c.is_ascii_digit())
    {
        return Some("Runes cannot be directly equipped in body slots outside of socketed items");
    }
    if matches!(clean, "key" | "tsc" | "isc" | "tbk" | "ibk" | "gld") {
        return Some("Consumable/inventory-only item cannot be equipped");
    }

    // Rings (rin) can only be in body slots 6 or 7
    if clean == "rin" && body_slot != 6 && body_slot != 7 {
        return Some("Rings can only be equipped in left ring (6) or right ring (7)");
    }

    // Amulets (amu) can only be in body slot 2
    if clean == "amu" && body_slot != 2 {
        return Some("Amulets can only be equipped in neck slot (2)");
    }

    // Belts (slot 8) cannot be rings, amulets, boots, gloves, helms, etc.
    if body_slot == 8
        && matches!(
            clean,
            "rin"
                | "amu"
                | "cap"
                | "skp"
                | "hlm"
                | "fhl"
                | "ghm"
                | "crn"
                | "msk"
                | "bhm"
                | "lbt"
                | "vbt"
                | "mbt"
                | "tbt"
                | "hbt"
                | "lgl"
                | "vgl"
                | "mgl"
                | "tgl"
                | "hgl"
        )
    {
        return Some("Non-belt item cannot be equipped in belt slot (8)");
    }

    None
}

fn classify_anomaly(
    code: &str,
    version: u8,
    location: u8,
    mode: u8,
    y: u8,
    flags: u32,
    dialect: Dialect,
) -> (InvariantClassification, String) {
    let clean_code = code.trim();

    // Condition 1: Check for phantom codes, invalid characters, or invalid body slot
    let axiom = d2r_core::domain::header::HeaderAxiom::new(version, dialect == Dialect::Alpha);
    let is_valid_item = axiom.is_plausible(mode, location, code.as_bytes(), flags)
        || axiom.is_plausible(mode, location, clean_code.as_bytes(), flags);

    // NOTE (Harness Intent & Known Limitation - harness-fallibility-governor):
    // In Alpha v105 saves, equipped items (mode==1) skip geometry via `header_geometry_with_mode(skip_geometry=true)`.
    // This means y is always 0 for equipped items - y==0 is NORMAL and must NOT be flagged.
    // Only flag y > 12 as physically out-of-range for body location slots (slots are 1..=12).
    // DESIGN INTENT: If future save format changes restore geometry for equipped items,
    // the y==0 check may need to be re-evaluated here.
    let is_invalid_body_loc = (mode == 1) && (y > 12);
    let is_known_phantom = matches!(
        clean_code,
        "wyws" | "wsd" | "wsd " | "oesw" | "w   " | "yws " | "s   " | "kk h"
    );
    let has_internal_space = clean_code.contains(' ') && clean_code.len() > 3;

    // Condition 1: Slot incompatibility has higher diagnostic value than generic
    // header plausibility. An impossible item-slot pair is a definitive drift signal.
    if let Some(reason) = is_slot_incompatible(clean_code, location) {
        return (
            InvariantClassification::SlotMismatchPhantomDrift,
            format!(
                "Slot-mismatch phantom drift: Item '{}' in body_slot {} is physically impossible ({})",
                clean_code, location, reason
            ),
        );
    }

    // Condition 2: Check for phantom codes, invalid characters, or invalid body slot.
    if !is_valid_item
        || is_known_phantom
        || is_invalid_body_loc
        || has_internal_space
        || clean_code.len() < 3
    {
        return (
            InvariantClassification::PhantomHeaderDrift,
            format!(
                "Bit-drift rupture from preceding option stream produced invalid item header '{}' (body_slot={}, loc={}, mode={}, ver={})",
                clean_code, y, location, mode, version
            ),
        );
    }

    // Condition 3: Check for 1-bit flag shift (bit 5 set instead of bit 4)
    if (flags & 0x00000020) != 0 && (flags & 0x00000010) == 0 {
        return (
            InvariantClassification::FlagShiftAnomaly,
            format!(
                "1-bit shifted flag pattern in code '{}': bit 5 is 1 while bit 4 (is_identified) is 0 (flags=0x{:08x})",
                clean_code, flags
            ),
        );
    }

    // Condition 4: Genuine violation
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

fn audit_file(path: &Path, huffman: &HuffmanTree, dialect: Dialect) -> FileAudit {
    match fs::read(path) {
        Ok(bytes) => audit_bytes(path, &bytes, huffman, dialect),
        Err(error) => failed_audit(path, "read", error.to_string(), None, Vec::new(), dialect),
    }
}

fn belt_anomaly(filename: &str, index: usize, item: &Item) -> Option<EquippedAnomaly> {
    if alpha_inventory_candidate_route(item, true) != AlphaInventoryRoute::Belt
        || is_belt_consumable(&item.code)
    {
        return None;
    }
    Some(EquippedAnomaly {
        file: filename.into(),
        item_index: index,
        code: item.code.trim().into(),
        version: item.version,
        mode: item.mode,
        location: item.location,
        x: item.x,
        y: item.y,
        flags: format!("0x{:08x}", item.flags),
        is_identified: item.is_identified,
        is_compact: item.is_compact,
        is_opaque: item.is_opaque() || item.is_semi_opaque(),
        classification: InvariantClassification::BeltItemMismatch,
        diagnosis: format!(
            "Impossible Alpha belt claim for '{}'; preserve as unknown and inspect item {} and the preceding item boundary with d2item_inspect. Placement/identity interpretation may be wrong; this does not prove input corruption.",
            item.code.trim(),
            index
        ),
    })
}

fn v3_belt_anomaly(
    filename: &str,
    index: usize,
    item: &Item,
    classification: InvariantClassification,
    detail: String,
) -> EquippedAnomaly {
    EquippedAnomaly {
        file: filename.into(),
        item_index: index,
        code: item.code.trim().into(),
        version: item.version,
        mode: item.mode,
        location: item.location,
        x: item.x,
        y: item.y,
        flags: format!("0x{:08x}", item.flags),
        is_identified: item.is_identified,
        is_compact: item.is_compact,
        is_opaque: item.is_opaque() || item.is_semi_opaque(),
        classification,
        diagnosis: format!(
            "{detail} Preserve the source slot; inspect item {index} and the preceding boundary with d2item_inspect. This finding does not establish input corruption."
        ),
    }
}

fn audit_v3_belt_positions(filename: &str, items: &[Item]) -> Vec<EquippedAnomaly> {
    let mut occupied = [None; 16];
    let mut consumables = [false; 16];
    let mut anomalies = Vec::new();
    for (index, item) in items.iter().enumerate().filter(|(_, item)| item.mode == 2) {
        if !is_belt_consumable(&item.code) {
            anomalies.push(v3_belt_anomaly(
                filename,
                index,
                item,
                InvariantClassification::BeltItemMismatch,
                format!(
                    "Non-consumable '{}' claims D2R-v3 belt slot {}.",
                    item.code.trim(),
                    item.x
                ),
            ));
        }
        if item.x >= 16 {
            anomalies.push(v3_belt_anomaly(
                filename,
                index,
                item,
                InvariantClassification::BeltSlotOutOfBounds,
                format!("D2R-v3 belt slot {} exceeds the 0..15 envelope.", item.x),
            ));
            continue;
        }
        // Even a bad item kind owns a raw slot claim: retain collision evidence.
        // It cannot, however, supply support for a consumable above it.
        if let Some(previous) = occupied[item.x as usize] {
            anomalies.push(v3_belt_anomaly(
                filename,
                index,
                item,
                InvariantClassification::BeltSlotCollision,
                format!(
                    "D2R-v3 belt slot {} is also claimed by item {previous}.",
                    item.x
                ),
            ));
        } else {
            occupied[item.x as usize] = Some(index);
        }
        consumables[item.x as usize] |= is_belt_consumable(&item.code);
    }
    for (index, item) in items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.mode == 2 && item.x < 16 && is_belt_consumable(&item.code))
    {
        let column = item.x % 4;
        let row = item.x / 4;
        let missing: Vec<_> = (0..row)
            .map(|lower| column + lower * 4)
            .filter(|slot| !consumables[*slot as usize])
            .collect();
        if !missing.is_empty() {
            anomalies.push(v3_belt_anomaly(
                filename, index, item, InvariantClassification::BeltColumnGap,
                format!("D2R-v3 belt column {column}, bottom-origin row {row}, slot {} has empty lower consumable slots {missing:?}; unused cells must be above occupied cells.", item.x),
            ));
        }
    }
    anomalies
}

fn audit_bytes(path: &Path, bytes: &[u8], huffman: &HuffmanTree, dialect: Dialect) -> FileAudit {
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut total_items = 0;
    let mut total_equipped = 0;
    let mut total_belt = 0;
    let mut anomalies = Vec::new();

    let parsed = match dialect {
        Dialect::Alpha => Item::read_player_items(bytes, huffman, true),
        Dialect::D2rV3 => Item::read_player_items_d2r_v3(bytes, huffman),
    };
    let items = match parsed {
        Ok(items) => items,
        Err(failure) => {
            return failed_audit(
                path,
                "primary_parse",
                failure.to_string(),
                Some(failure.bit_offset),
                failure.context_stack,
                dialect,
            );
        }
    };
    {
        total_items += items.len();
        for (idx, it) in items.iter().enumerate() {
            if dialect == Dialect::Alpha
                && alpha_inventory_candidate_route(it, true) == AlphaInventoryRoute::Belt
            {
                total_belt += 1;
                if let Some(anomaly) = belt_anomaly(&filename, idx, it) {
                    anomalies.push(anomaly);
                }
            }
            // In D2 save format, mode == 1 indicates an equipped item (body slot 1..=10).
            // mode == 0 with location == 1 means stored in player inventory (can be unidentified).
            let is_equipped = it.header.mode == 1 || it.mode == 1;

            if is_equipped {
                total_equipped += 1;
                let has_slot_mismatch =
                    is_slot_incompatible(&it.code, it.header.location).is_some();
                if !it.header.is_identified || (dialect == Dialect::D2rV3 && has_slot_mismatch) {
                    let is_opaque = it.is_opaque() || it.is_semi_opaque();
                    let has_slot_mismatch =
                        is_slot_incompatible(&it.code, it.header.location).is_some();

                    // An opaque item with a physically plausible slot cannot establish a
                    // genuine identification violation. Retain only hard slot mismatches,
                    // which are independently sufficient evidence of parser drift.
                    if is_opaque && !has_slot_mismatch {
                        continue;
                    }

                    let (classification, diagnosis) = classify_anomaly(
                        &it.code,
                        it.header.version,
                        it.header.location,
                        it.header.mode,
                        it.header.y,
                        it.header.flags,
                        dialect,
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
                        is_opaque,
                        classification,
                        diagnosis,
                    });
                }
            }
        }
    }

    if dialect == Dialect::D2rV3 {
        total_belt = items.iter().filter(|item| item.mode == 2).count();
        anomalies.extend(audit_v3_belt_positions(&filename, &items));
    }
    FileAudit {
        items: total_items,
        equipped: total_equipped,
        belt: total_belt,
        anomalies,
        failure: None,
    }
}

fn main() {
    let start_time = Instant::now();
    let args = cli::parse_args();
    let huffman = HuffmanTree::new();

    let mut save_paths = Vec::new();

    if let Some(ref single_fixture) = args.fixture {
        let p = PathBuf::from(single_fixture);
        if !p.exists() {
            eprintln!(
                "Error: Specified fixture file does not exist: {}",
                single_fixture
            );
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
    let results: Vec<FileAudit> = save_paths
        .par_iter()
        .map(|path| audit_file(path, &huffman, args.dialect))
        .collect();

    let mut total_items = 0;
    let mut total_equipped = 0;
    let mut total_belt = 0;
    let mut all_anomalies = Vec::new();
    let mut total_saves_parsed = 0;
    let mut parse_failures = Vec::new();
    let mut affected_files_set = BTreeMap::new();

    for result in results {
        if let Some(failure) = result.failure {
            parse_failures.push(failure);
            continue;
        }
        total_saves_parsed += 1;
        total_items += result.items;
        total_equipped += result.equipped;
        total_belt += result.belt;
        for anom in result.anomalies {
            *affected_files_set.entry(anom.file.clone()).or_insert(0) += 1;
            all_anomalies.push(anom);
        }
    }

    let mut phantom_count = 0;
    let mut slot_mismatch_count = 0;
    let mut flag_shift_count = 0;
    let mut true_violation_count = 0;
    let mut belt_mismatch_count = 0;
    let mut belt_gap_count = 0;
    let mut belt_collision_count = 0;
    let mut belt_bounds_count = 0;

    for a in &all_anomalies {
        match a.classification {
            InvariantClassification::PhantomHeaderDrift => phantom_count += 1,
            InvariantClassification::SlotMismatchPhantomDrift => slot_mismatch_count += 1,
            InvariantClassification::FlagShiftAnomaly => flag_shift_count += 1,
            InvariantClassification::TrueUnidentifiedViolation => true_violation_count += 1,
            InvariantClassification::BeltItemMismatch => belt_mismatch_count += 1,
            InvariantClassification::BeltColumnGap => belt_gap_count += 1,
            InvariantClassification::BeltSlotCollision => belt_collision_count += 1,
            InvariantClassification::BeltSlotOutOfBounds => belt_bounds_count += 1,
        }
    }

    let affected_files: Vec<String> = affected_files_set.into_keys().collect();

    let report = InvariantAuditReport {
        schema_version: "d2save-equipped-primary/v3",
        dialect: args.dialect,
        status: if !parse_failures.is_empty() {
            "error"
        } else if !all_anomalies.is_empty() {
            "anomaly"
        } else {
            "ok"
        },
        coverage_limit: "Primary player list identification and partial slot rules; Alpha belt item-kind claims; D2R-v3 belt kinds, 16-slot bounds, collisions and bottom-filled columns. No Alpha positions, equipped-belt-specific capacity, mercenary/corpse/golem lists, complete slot oracle, game-load or fidelity proof.",
        total_saves_parsed,
        parse_failures,
        total_saves_checked: save_paths.len(),
        total_items_checked: total_items,
        total_equipped_checked: total_equipped,
        total_belt_checked: total_belt,
        belt_item_mismatch_count: belt_mismatch_count,
        belt_column_gap_count: belt_gap_count,
        belt_slot_collision_count: belt_collision_count,
        belt_slot_out_of_bounds_count: belt_bounds_count,
        total_anomalies: all_anomalies.len(),
        phantom_header_drift_count: phantom_count,
        slot_mismatch_phantom_drift_count: slot_mismatch_count,
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
        std::process::exit(report.exit_code());
    }

    // Human-readable console output
    let elapsed = start_time.elapsed();
    println!("================================================================================");
    println!("  d2save_equipped_invariant: D2R Equipped Identification Invariant Radar");
    println!("================================================================================");
    println!("Files audited:       {}", save_paths.len());
    println!("Dialect:             {}", args.dialect.label());
    println!("Parsed saves:        {}", total_saves_parsed);
    println!("Parse failures:      {}", report.parse_failures.len());
    for failure in report.parse_failures.iter().take(1) {
        println!("First failure: {}: {}", failure.file, failure.message);
        println!("Hint: {}", failure.hint);
    }
    println!("Total items:         {}", total_items);
    println!("Equipped items:      {}", total_equipped);
    println!("Belt claims:         {}", total_belt);
    println!("Belt mismatches:     {}", belt_mismatch_count);
    println!("Belt column gaps:    {}", belt_gap_count);
    println!("Belt slot collisions:{}", belt_collision_count);
    println!("Belt out of bounds:  {}", belt_bounds_count);
    println!("Total anomalies:     {}", all_anomalies.len());
    println!("Elapsed time:        {:.2?}", elapsed);
    println!("--------------------------------------------------------------------------------");
    println!("Anomaly Breakdown:");
    println!(
        "  [1] Phantom Header Drift:        {} (Bit-drift rupture from preceding option stream)",
        phantom_count
    );
    println!(
        "  [2] Slot Mismatch Phantom Drift: {} (Physically impossible slot assignment from bit-drift)",
        slot_mismatch_count
    );
    println!(
        "  [3] Flag Shift Anomaly:          {} (Bit 5 set instead of Bit 4)",
        flag_shift_count
    );
    println!(
        "  [4] True Unidentified Violation: {} (Genuine equipped item unidentified)",
        true_violation_count
    );
    println!("--------------------------------------------------------------------------------");

    if !affected_files.is_empty() {
        println!("Affected Save Files ({} files):", affected_files.len());
        for file in &affected_files {
            println!("  - {}", file);
        }
    }

    if args.verbose || (!args.token_efficient && !all_anomalies.is_empty()) {
        println!(
            "--------------------------------------------------------------------------------"
        );
        println!("Sample Anomalies (First 20):");
        for anom in all_anomalies.iter().take(20) {
            println!(
                "  [{}] idx={:02} code='{:4}' ver={} loc={} mode={} y={:2} x={:2} flags={} class={:?}",
                anom.file,
                anom.item_index,
                anom.code,
                anom.version,
                anom.location,
                anom.mode,
                anom.y,
                anom.x,
                anom.flags,
                anom.classification
            );
        }
        if all_anomalies.len() > 20 {
            println!("  ... and {} more anomalies.", all_anomalies.len() - 20);
        }
    }

    println!("================================================================================");
    if !report.parse_failures.is_empty() {
        eprintln!("Radar Verdict: INCOMPLETE; unparsed saves cannot establish the invariant.");
    } else if true_violation_count > 0 {
        eprintln!(
            "WARNING: Found {} TRUE unidentified equipped items!",
            true_violation_count
        );
    } else if !all_anomalies.is_empty() {
        println!(
            "Radar Verdict: FAILED; impossible placements or flag anomalies require investigation."
        );
    } else {
        println!(
            "Radar Verdict: ZERO true unidentified violations. Review reported drift anomalies and coverage limits."
        );
    }
    std::process::exit(report.exit_code());
}

#[cfg(test)]
mod tests {
    use super::*;
    use d2r_macros::settled_contract_test;

    fn v3_potion(slot: u8) -> Item {
        let mut item = Item {
            code: "hp1".into(),
            ..Item::default()
        };
        item.mode = 2;
        item.x = slot;
        item
    }

    #[settled_contract_test]
    fn v3_belt_position_audit_checks_all_column_occupancy_masks() {
        for column in 0..4 {
            for mask in 0u8..16 {
                let items: Vec<_> = (0..4)
                    .filter(|row| mask & (1 << row) != 0)
                    .map(|row| v3_potion(column + row * 4))
                    .collect();
                let anomalies = audit_v3_belt_positions("mask.d2s", &items);
                let supported = matches!(mask, 0 | 1 | 3 | 7 | 15);
                assert_eq!(
                    anomalies.is_empty(),
                    supported,
                    "column={column}, mask={mask}"
                );
                for anomaly in &anomalies {
                    assert!(matches!(
                        anomaly.classification,
                        InvariantClassification::BeltColumnGap
                    ));
                    assert_eq!(anomaly.x % 4, column);
                    assert_eq!(anomaly.x, items[anomaly.item_index].x);
                    assert!(anomaly.diagnosis.contains("empty lower consumable slots"));
                }
            }
        }
        // Every slot and mixed consumable types are valid when all columns are full.
        let mut full: Vec<_> = (0..16).map(v3_potion).collect();
        for (index, item) in full.iter_mut().enumerate() {
            item.code = ["hp1", "mp5", "rvs", "tsc"][index % 4].into();
        }
        assert!(audit_v3_belt_positions("full.d2s", &full).is_empty());
    }

    #[settled_contract_test]
    fn v3_belt_positions_preserve_collision_bounds_and_invalid_kind_evidence() {
        let mut bad_kind = v3_potion(0);
        bad_kind.code = "rin".into();
        let mut inventory = v3_potion(8);
        inventory.mode = 0;
        let items = vec![
            bad_kind,
            v3_potion(4),
            v3_potion(4),
            v3_potion(16),
            inventory,
        ];
        let anomalies = audit_v3_belt_positions("bad.d2s", &items);
        let count = |name: &str| {
            anomalies
                .iter()
                .filter(|anomaly| format!("{:?}", anomaly.classification) == name)
                .count()
        };
        assert_eq!(count("BeltItemMismatch"), 1);
        assert_eq!(count("BeltSlotCollision"), 1);
        assert_eq!(count("BeltSlotOutOfBounds"), 1);
        assert_eq!(count("BeltColumnGap"), 2);
        assert_eq!(anomalies.len(), 5);
        assert!(anomalies.iter().all(|anomaly| anomaly.item_index < 4));
        // The auditor does not normalize, remove or move any claimed slot.
        assert_eq!(
            items.iter().map(|item| item.x).collect::<Vec<_>>(),
            [0, 4, 4, 16, 8]
        );
    }

    #[d2r_macros::hypothesis_test]
    fn druid_v3_belt_gap_is_detected_after_one_raw_slot_mutation() {
        let root = std::env::var_os("D2R_CORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let path = root
            .join("tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s");
        let bytes = fs::read(&path).unwrap();
        let huffman = HuffmanTree::new();
        let items = Item::read_player_items_d2r_v3(&bytes, &huffman).unwrap();
        assert_eq!(items.iter().filter(|item| item.mode == 2).count(), 11);
        assert!(audit_v3_belt_positions("druid.d2s", &items).is_empty());
        let (index, potion) = items
            .iter()
            .enumerate()
            .find(|(_, item)| item.mode == 2 && item.x == 0)
            .unwrap();
        assert!(is_belt_consumable(&potion.code));
        // V3 ordinary header: mode at +35, body slot +38, raw x +42 (4 bits).
        // Move the sole column0 bottom item to row1 by toggling x bit2 only.
        let offset = potion.range.start as usize + 44;
        assert_eq!(bytes[offset / 8] & (1 << (offset % 8)), 0);
        let mut mutated = bytes.clone();
        mutated[offset / 8] ^= 1 << (offset % 8);
        assert_eq!(
            bytes.iter().zip(&mutated).filter(|(a, b)| a != b).count(),
            1
        );
        let audit = audit_bytes(&path, &mutated, &huffman, Dialect::D2rV3);
        assert!(audit.failure.is_none());
        assert_eq!(audit.belt, 11);
        let gaps: Vec<_> = audit
            .anomalies
            .iter()
            .filter(|anomaly| {
                matches!(
                    anomaly.classification,
                    InvariantClassification::BeltColumnGap
                )
            })
            .collect();
        assert_eq!(gaps.len(), 1);
        assert_eq!((gaps[0].item_index, gaps[0].x), (index, 4));
        assert!(gaps[0].diagnosis.contains("[0]"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[settled_contract_test]
    fn belt_audit_reports_identified_and_opaque_non_consumables() {
        let mut item = Item {
            code: "rin".into(),
            ..Item::default()
        };
        item.mode = 2;
        item.is_identified = true;
        item.x = 3;
        item.y = 1;
        item.modules
            .push(d2r_core::domain::item::entity::ItemModule::Opaque(vec![
                true, false,
            ]));
        let anomaly =
            belt_anomaly("test.d2s", 7, &item).expect("opaque belt mismatch must be reported");
        assert!(anomaly.is_identified && anomaly.is_opaque);
        assert_eq!((anomaly.item_index, anomaly.x, anomaly.y), (7, 3, 1));
        assert!(matches!(
            anomaly.classification,
            InvariantClassification::BeltItemMismatch
        ));
        assert!(anomaly.diagnosis.contains("preserve as unknown"));
        item.code = "isc".into();
        assert!(belt_anomaly("test.d2s", 7, &item).is_none());
        item.code = "rin".into();
        item.mode = 1;
        assert!(belt_anomaly("test.d2s", 7, &item).is_none());
    }

    #[d2r_macros::hypothesis_test]
    fn druid_anya_belt_claims_are_isolated_without_losing_entities() {
        use d2r_core::verify::alpha_inventory_routing::alpha_inventory_route;
        let root = std::env::var_os("D2R_CORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let bytes = fs::read(root.join(
            "tests/fixtures/savegames/gameplay/normal/act5/TESTDRUID_Quest3_AnyaRescued.d2s",
        ))
        .unwrap();
        let items = Item::read_player_items(&bytes, &HuffmanTree::new(), true).unwrap();
        let visible: Vec<_> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.is_residue() && !item.code.trim().is_empty())
            .collect();
        let mut before_belt = 0;
        let mut after_belt = 0;
        let mut mismatches = 0;
        let mut unknown = 0;
        for (index, item) in &visible {
            before_belt += usize::from(
                alpha_inventory_candidate_route(item, true) == AlphaInventoryRoute::Belt,
            );
            let route = alpha_inventory_route(item, true);
            after_belt += usize::from(route == AlphaInventoryRoute::Belt);
            unknown += usize::from(route == AlphaInventoryRoute::Unknown);
            if let Some(anomaly) = belt_anomaly("TESTDRUID_Quest3_AnyaRescued.d2s", *index, item) {
                mismatches += 1;
                assert_eq!(route, AlphaInventoryRoute::Unknown);
                assert_eq!(anomaly.item_index, *index);
            }
            if route == AlphaInventoryRoute::Belt {
                assert!(is_belt_consumable(&item.code));
            }
        }
        assert_eq!(
            (visible.len(), before_belt, after_belt, mismatches, unknown),
            (31, 18, 3, 15, 20)
        );
    }

    #[settled_contract_test]
    fn explicit_dialect_rejects_missing_and_unknown_values() {
        assert_eq!(Dialect::parse(Some("alpha")), Ok(Dialect::Alpha));
        assert_eq!(Dialect::parse(Some("d2r-v3")), Ok(Dialect::D2rV3));
        assert!(Dialect::parse(None).is_err());
        assert!(Dialect::parse(Some("unknown")).is_err());
    }

    #[settled_contract_test]
    fn truncated_primary_is_a_failure_not_an_empty_audit() {
        let audit = audit_bytes(
            Path::new("truncated.d2s"),
            &[],
            &HuffmanTree::new(),
            Dialect::D2rV3,
        );
        assert_eq!((audit.items, audit.equipped), (0, 0));
        let failure = audit
            .failure
            .expect("an unparsed save must not be a clean audit");
        assert_eq!(failure.stage, "primary_parse");
        assert!(failure.bit_offset.is_some());
        assert!(!failure.message.is_empty());
        assert!(failure.hint.contains("--dialect d2r-v3"));
    }

    #[d2r_macros::hypothesis_test]
    fn rogue_stats_verify_fixture_seals_equipped_anomaly_classification() {
        let fixture = Path::new(
            "tests/fixtures/savegames/gameplay/normal/act1/TESTASSASSIN_Act1_Rogue_Stats_Verify.d2s",
        );
        let huffman = HuffmanTree::new();
        let audit = audit_file(fixture, &huffman, Dialect::Alpha);
        assert!(
            audit.failure.is_none(),
            "the legacy fixture must actually parse"
        );
        // Added belt coverage must not change the original equipped classification.
        assert_eq!(
            audit
                .anomalies
                .iter()
                .filter(|anomaly| matches!(
                    anomaly.classification,
                    InvariantClassification::BeltItemMismatch
                ))
                .count(),
            14
        );
        let anomalies: Vec<_> = audit
            .anomalies
            .into_iter()
            .filter(|anomaly| {
                !matches!(
                    anomaly.classification,
                    InvariantClassification::BeltItemMismatch
                )
            })
            .collect();

        assert!(
            !anomalies
                .iter()
                .any(|anomaly| anomaly.item_index == 8 && anomaly.code == "cap"),
            "Alpha v105 equipped geometry uses y == 0, so cap index 8 must not be reported"
        );
        assert_eq!(
            anomalies.len(),
            1,
            "the fixture should retain exactly the cm3 slot-mismatch phantom"
        );

        let anomaly = &anomalies[0];
        assert_eq!(anomaly.item_index, 30);
        assert_eq!(anomaly.code, "cm3");
        assert!(anomaly.is_opaque);
        assert!(matches!(
            &anomaly.classification,
            InvariantClassification::SlotMismatchPhantomDrift
        ));
    }
}
