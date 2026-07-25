// d2item_opaque_census.rs -- Report-only corpus census harness for opaque predicate hit routing
// Standalone verification binary for collecting item opaque predicate hits across a save corpus.

use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use d2r_core::item::{HuffmanTree, Item, ItemClassProjection};
use d2r_core::verify::args::{ArgError, ArgParser};
use serde::Serialize;

#[derive(Serialize, Debug, Default)]
struct ClassificationCounts {
    semi_opaque: usize,
    residue: usize,
    opaque: usize,
}

#[derive(Serialize, Debug)]
struct UnreadableSave {
    save_file: String,
    reason: String,
}

#[derive(Serialize, Debug)]
struct CensusRow {
    save_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    section: Option<String>,
    item_index: usize,
    item_path: String,
    code: String,
    bit_start: u64,
    bit_end: u64,
    total_bits: u64,
    classification: String,
    is_semi_opaque: bool,
    is_residue: bool,
    is_opaque: bool,
}

#[derive(Serialize, Debug)]
struct OpaqueCensusReport {
    saves_dir: String,
    collection_policy: String,
    total_saves_scanned: usize,
    total_items_collected: usize,
    cache_ineligible_count: usize,
    opaque_predicate_hit_count: usize,
    classification_counts: ClassificationCounts,
    unreadable_saves: Vec<UnreadableSave>,
    rows: Vec<CensusRow>,
}

struct CollectedItemInfo<'a> {
    item: &'a Item,
    section: Option<String>,
    item_index: usize,
    item_path: String,
}

fn collect_items_recursive<'a>(
    item: &'a Item,
    section: Option<String>,
    parent_path: &str,
    save_item_counter: &mut usize,
    collected: &mut Vec<CollectedItemInfo<'a>>,
) {
    let item_index = *save_item_counter;
    *save_item_counter += 1;

    collected.push(CollectedItemInfo {
        item,
        section,
        item_index,
        item_path: parent_path.to_string(),
    });

    for (s_idx, socketed) in item.socketed_items.iter().enumerate() {
        let child_path = format!("{}/socket/{}", parent_path, s_idx);
        collect_items_recursive(
            socketed,
            collected.last().and_then(|info| info.section.clone()),
            &child_path,
            save_item_counter,
            collected,
        );
    }
}

fn main() -> Result<()> {
    let mut parser = ArgParser::new("d2item_opaque_census")
        .description("Report-only corpus census harness for opaque predicate hit routing");

    parser
        .add_opt("saves-dir", "Directory containing .d2s save files")
        .short('d')
        .long("saves-dir")
        .default("tests/fixtures/savegames/modified");

    parser
        .add_flag("json", "Emit opaque census report in JSON format to stdout")
        .short('j')
        .long("json");

    parser
        .add_opt("output", "Path to output JSON report file")
        .short('o')
        .long("output");

    let args: Vec<_> = env::args_os().skip(1).collect();
    let parsed = match parser.parse(args) {
        Ok(p) => p,
        Err(ArgError::Help(h)) => {
            println!("{}", h);
            process::exit(0);
        }
        Err(ArgError::Error(e)) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let saves_dir_str = parsed
        .get("saves-dir")
        .cloned()
        .unwrap_or_else(|| "tests/fixtures/savegames/modified".to_string());

    let saves_dir_path = Path::new(&saves_dir_str);
    if !saves_dir_path.exists() || !saves_dir_path.is_dir() {
        eprintln!(
            "[ERROR] Saves directory '{}' does not exist or is not a directory.",
            saves_dir_str
        );
        process::exit(1);
    }

    let mut save_files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(saves_dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("d2s") {
                save_files.push(path);
            }
        }
    }
    save_files.sort();

    let total_saves_scanned = save_files.len();
    let huffman = HuffmanTree::new();

    let mut unreadable_saves = Vec::new();
    let mut total_items_collected = 0usize;
    let mut cache_ineligible_count = 0usize;
    let mut opaque_predicate_hit_count = 0usize;
    let mut classification_counts = ClassificationCounts::default();
    let mut rows = Vec::new();

    for save_path in &save_files {
        let save_file_rel = save_path.display().to_string();
        let bytes = match fs::read(save_path) {
            Ok(b) => b,
            Err(e) => {
                unreadable_saves.push(UnreadableSave {
                    save_file: save_file_rel,
                    reason: format!("File read error: {}", e),
                });
                continue;
            }
        };

        if bytes.len() < 8 {
            unreadable_saves.push(UnreadableSave {
                save_file: save_file_rel,
                reason: "File buffer under 8 bytes".to_string(),
            });
            continue;
        }

        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let is_alpha = version == 105;

        let mut save_item_counter = 0usize;

        match Item::read_player_items(&bytes, &huffman, is_alpha) {
            Ok(top_items) => {
                for (t_idx, top_item) in top_items.iter().enumerate() {
                    let mut collected = Vec::new();
                    let top_path = format!("{}", t_idx);
                    collect_items_recursive(
                        top_item,
                        Some("Player Items".to_string()),
                        &top_path,
                        &mut save_item_counter,
                        &mut collected,
                    );

                    for info in collected {
                        process_collected_item(
                            info,
                            &save_file_rel,
                            &mut total_items_collected,
                            &mut cache_ineligible_count,
                            &mut opaque_predicate_hit_count,
                            &mut classification_counts,
                            &mut rows,
                        );
                    }
                }
            }
            Err(e) => {
                unreadable_saves.push(UnreadableSave {
                    save_file: save_file_rel,
                    reason: format!("Failed to parse items from save: {}", e),
                });
            }
        }
    }

    let report = OpaqueCensusReport {
        saves_dir: saves_dir_str,
        collection_policy: "recursive (top-level and socketed player items)".to_string(),
        total_saves_scanned,
        total_items_collected,
        cache_ineligible_count,
        opaque_predicate_hit_count,
        classification_counts,
        unreadable_saves,
        rows,
    };

    let report_json = serde_json::to_string_pretty(&report)
        .context("Failed to serialize opaque census report to JSON")?;

    if let Some(output_path) = parsed.get("output") {
        fs::write(output_path, &report_json)
            .with_context(|| format!("Cannot write output report to '{}'", output_path))?;
    }

    if parsed.is_set("json") {
        println!("{}", report_json);
    } else if parsed.get("output").is_none() {
        println!(
            "Scanned {} saves: {} total items, {} cache-ineligible, {} opaque predicate hits (semi_opaque: {}, residue: {}, opaque: {}).",
            report.total_saves_scanned,
            report.total_items_collected,
            report.cache_ineligible_count,
            report.opaque_predicate_hit_count,
            report.classification_counts.semi_opaque,
            report.classification_counts.residue,
            report.classification_counts.opaque,
        );
        if !report.unreadable_saves.is_empty() {
            println!("Unreadable saves: {}", report.unreadable_saves.len());
        }
    }

    Ok(())
}

fn process_collected_item(
    info: CollectedItemInfo,
    save_file_rel: &str,
    total_items_collected: &mut usize,
    cache_ineligible_count: &mut usize,
    opaque_predicate_hit_count: &mut usize,
    classification_counts: &mut ClassificationCounts,
    rows: &mut Vec<CensusRow>,
) {
    *total_items_collected += 1;

    let item = info.item;
    if ItemClassProjection::extract(item).is_none() {
        *cache_ineligible_count += 1;
    }

    let is_semi_opaque = item.is_semi_opaque();
    let is_residue = item.is_residue();
    let is_opaque = item.is_opaque();

    // Mutually exclusive classification according to Mini-Spec precedence
    let classification = if is_semi_opaque {
        Some("semi_opaque")
    } else if is_residue {
        Some("residue")
    } else if is_opaque {
        Some("opaque")
    } else {
        None
    };

    if let Some(cls) = classification {
        *opaque_predicate_hit_count += 1;
        match cls {
            "semi_opaque" => classification_counts.semi_opaque += 1,
            "residue" => classification_counts.residue += 1,
            "opaque" => classification_counts.opaque += 1,
            _ => {}
        }

        rows.push(CensusRow {
            save_file: save_file_rel.to_string(),
            section: info.section,
            item_index: info.item_index,
            item_path: info.item_path,
            code: item.code.trim().to_string(),
            bit_start: item.range.start,
            bit_end: item.range.end,
            total_bits: item.total_bits as u64,
            classification: cls.to_string(),
            is_semi_opaque,
            is_residue,
            is_opaque,
        });
    }
}
